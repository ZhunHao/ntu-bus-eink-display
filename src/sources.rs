//! Arrival data sources.
//!
//! `NtuOmnibus`  - the NTU Omnibus app's backend (see ENDPOINTS.md). Unofficial: the
//!                 apiVersion hashes change whenever NTU redeploys the app.
//! `LtaDataMall` - LTA's official v3 BusArrival API (needs an AccountKey).
//!
//! Every source returns the next buses' arrival times, soonest first. An empty list means no
//! bus is currently estimated.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Duration as Span, FixedOffset};
use serde_json::{json, Value};

use crate::Result;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into()
}

/// NTU sends numbers as strings, and "-" when there is no estimate.
fn as_int(v: &Value) -> Option<i64> {
    v.as_str().and_then(|s| s.parse().ok()).or_else(|| v.as_i64())
}

pub struct NtuOmnibus {
    agent: ureq::Agent,
    module_version: Option<String>,
    /// apiVersion hashes found in NTU's live scripts, replacing the built-in ones after a redeploy
    api_versions: HashMap<String, String>,
    /// When an action's apiVersion lookup last failed, or found a hash NTU still rejected.
    /// The lookup downloads the app's scripts, so it isn't repeated on every fetch.
    lookup_failed: HashMap<String, Instant>,
}

impl NtuOmnibus {
    const HOST: &'static str = "https://apps.ntu.edu.sg";
    const BASE: &'static str = "https://apps.ntu.edu.sg/NTUOmnibus";
    const CSRF: &'static str = "T6C+9iB49TLra4jEsMeSckDMNhQ=";
    const COOKIE: &'static str = "nr1Users=lid=Anonymous; nr2Users=crf=T6C%2b9iB49TLra4jEsMeSckDMNhQ%3d";
    const VIEW: &'static str = "MainFlow.Shuttle";
    // apiVersion hashes from ENDPOINTS.md (3.7 and 3.8). When NTU redeploys and these go
    // stale, fresh ones are read from the app's live JavaScript (see find_api_version).
    const FMS: (&'static str, &'static str) =
        ("/screenservices/CampusShuttle_MUI/ActionGetETAAndNextETA_FMS", "2vqnARteBS8UFZvhYCydcw");
    const LTA: (&'static str, &'static str) =
        ("/screenservices/CampusShuttle_MUI/ActionGetETAAndNextETA_LTA", "NBbTcPPSIJ15dc3NGRGWtA");
    // ENDPOINTS.md 3.4: a route's stops in order
    const ROUTE_STOPS: (&'static str, &'static str) = (
        "/screenservices/NTUMobile_MCW/NTU_Blocks/WB_ServiceDetail/DataActionGetBusServiceDetails",
        "1PUVaxmcJyMdguP9kAtsOw",
    );

    /// How long to wait before looking an action's apiVersion up again after a failed attempt
    const LOOKUP_RETRY: Duration = Duration::from_secs(15 * 60);

    pub fn new() -> Self {
        Self { agent: agent(), module_version: None, api_versions: HashMap::new(), lookup_failed: HashMap::new() }
    }

    fn module_version(&mut self, refresh: bool) -> Result<String> {
        if refresh || self.module_version.is_none() {
            let v: Value = self
                .agent
                .get(format!("{}/moduleservices/moduleversioninfo", Self::BASE))
                .call()?
                .body_mut()
                .read_json()?;
            let token = v["versionToken"].as_str().ok_or("NTU: no versionToken")?.to_string();
            eprintln!("NTU moduleVersion = {token}");
            self.module_version = Some(token);
        }
        Ok(self.module_version.clone().unwrap())
    }

    /// Calls an action with `inputParameters`.
    fn post(&mut self, endpoint: (&str, &str), params: Value) -> Result<Value> {
        self.post_raw(endpoint, Self::VIEW, json!({"inputParameters": params}))
    }

    /// Sends `body` plus the version info and view name every request needs.
    fn post_raw(&mut self, (path, built_in): (&str, &str), view: &str, mut body: Value) -> Result<Value> {
        let mut refresh_module = false;
        let mut looked_up_api = false;
        for _ in 0..3 {
            let api = self.api_versions.get(path).map_or(built_in, String::as_str).to_string();
            body["versionInfo"] = json!({"moduleVersion": self.module_version(refresh_module)?, "apiVersion": api});
            body["viewName"] = json!(view);
            let resp: Value = self
                .agent
                .post(format!("{}{path}", Self::BASE))
                .header("X-CSRFToken", Self::CSRF)
                .header("Cookie", Self::COOKIE)
                .send_json(&body)?
                .body_mut()
                .read_json()?;
            let info = &resp["versionInfo"];
            refresh_module = info["hasModuleVersionChanged"].as_bool() == Some(true);
            if info["hasApiVersionChanged"].as_bool() == Some(true) {
                if looked_up_api {
                    self.lookup_failed.insert(path.to_string(), Instant::now());
                    return Err(format!("NTU changed {path} and its new apiVersion doesn't work; see ENDPOINTS.md").into());
                }
                if self.lookup_failed.get(path).is_some_and(|at| at.elapsed() < Self::LOOKUP_RETRY) {
                    return Err(format!("NTU changed {path}; its apiVersion lookup failed, retrying later").into());
                }
                // NTU redeployed the app: read this action's new hash from the live scripts
                let fresh = match self.find_api_version(path) {
                    Ok(fresh) => fresh,
                    Err(e) => {
                        self.lookup_failed.insert(path.to_string(), Instant::now());
                        return Err(e);
                    }
                };
                eprintln!("NTU apiVersion for {path} is now {fresh}");
                self.api_versions.insert(path.to_string(), fresh);
                looked_up_api = true;
                refresh_module = true;
                continue;
            }
            if refresh_module {
                continue; // fetch the new moduleVersion and retry
            }
            self.lookup_failed.remove(path);
            return response_data(path, &resp);
        }
        Err("NTU: version info keeps changing".into())
    }

    /// Finds an action's current apiVersion in the app's JavaScript. The web app serves the same
    /// scripts as the APK, listed in its manifest, and each action call is written as
    /// `"screenservices/<path>", "<apiVersion>"`.
    fn find_api_version(&mut self, path: &str) -> Result<String> {
        let manifest: Value = self
            .agent
            .get(format!("{}/moduleservices/moduleinfo", Self::BASE))
            .call()?
            .body_mut()
            .read_json()?;
        let module = path.trim_start_matches("/screenservices/").split('/').next().unwrap_or_default();
        let prefix = format!("/NTUOmnibus/scripts/{module}.");
        let scripts = manifest["manifest"]["urlVersions"].as_object().ok_or("NTU: no script list in moduleinfo")?;
        let mut scripts: Vec<_> = scripts.iter().filter(|(k, _)| k.starts_with(&prefix) && k.ends_with(".js")).collect();
        // Server actions are called from the module's controller script, so read that one first
        scripts.sort_by_key(|(k, _)| !k.ends_with(".controller.js"));
        for (script, version) in scripts {
            let js = self
                .agent
                .get(format!("{}{script}{}", Self::HOST, version.as_str().unwrap_or_default()))
                .call()?
                .body_mut()
                .with_config()
                .limit(10 << 20)
                .read_to_string()?;
            if let Some(api) = api_version_in(&js, path) {
                return Ok(api);
            }
        }
        Err(format!("NTU: {path} is no longer in the app; it was renamed or removed (see ENDPOINTS.md)").into())
    }

    /// Campus shuttle (Red/Blue/Green/Brown/Grey) at an NTU stop name, e.g. "Hall 11 Blk 55".
    pub fn shuttle(&mut self, stop: &str, route: &str) -> Result<Vec<DateTime<FixedOffset>>> {
        let data = self.post(Self::FMS, json!({"BusStopCode": stop, "RouteCode": route}))?;
        parse_ntu_shuttle(&data, crate::now())
    }

    /// The stops a route serves, in order, as (stop to put in config.toml, stop name).
    /// Works for the campus shuttles and for 179/179A/199.
    pub fn route_stops(&mut self, route: &str) -> Result<Vec<(String, String)>> {
        let public = route.starts_with(|c: char| c.is_ascii_digit());
        let vars = json!({"BusRouteId": route, "RouteName": route, "IsSBS": public, "Route": route});
        let data = self.post_raw(
            Self::ROUTE_STOPS,
            "CommonFlow.SelectedRouteServiceDetail",
            json!({"screenData": {"variables": vars}, "clientVariables": {}}),
        )?;
        Ok(parse_route_stops(&data))
    }

    /// Public bus via NTU's copy of LTA data (only 179/179A/199 stops).
    pub fn public(&mut self, stop: &str, service: &str) -> Result<Vec<DateTime<FixedOffset>>> {
        let data = self.post(Self::LTA, json!({"BusStopCode": stop, "ServiceNo": service}))?;
        parse_ntu_public(&data, crate::now())
    }
}

/// The `data` of an action's response. NTU reports a failed action with HTTP 200, an empty
/// `data` and an `exception`, e.g. "No role validation found" for a wrong `viewName`.
fn response_data(path: &str, resp: &Value) -> Result<Value> {
    if let Some(e) = resp.get("exception") {
        let message = e["message"].as_str().map_or_else(|| e.to_string(), str::to_string);
        return Err(format!("NTU {path}: {message}").into());
    }
    match resp.get("data") {
        Some(d) => Ok(d.clone()),
        None => Err(format!("NTU {path}: unexpected response").into()),
    }
}

/// The furthest-off ETA taken at face value (10 hours); anything beyond it is bad data.
const MAX_ETA_SECONDS: i64 = 10 * 3600;

/// The two ETA fields of an NTU response. They are always sent, as a number, "-" (no estimate)
/// or "" (stop not on the route), so a missing one means NTU changed the response.
fn ntu_etas<'a>(data: &'a Value, what: &str) -> Result<[&'a Value; 2]> {
    match (data.get("ETA"), data.get("NextETA")) {
        (Some(eta), Some(next)) => Ok([eta, next]),
        _ => Err(format!("NTU {what}: no ETA in the response; NTU changed it (see ENDPOINTS.md)").into()),
    }
}

/// NTU gives shuttle ETAs in whole minutes from `now`, rounded down: "4" is 4 to 5 minutes
/// away. The middle of that minute is used, so the board shows the same number as NTU's app
/// until the next fetch instead of one less.
fn parse_ntu_shuttle(data: &Value, now: DateTime<FixedOffset>) -> Result<Vec<DateTime<FixedOffset>>> {
    Ok(ntu_etas(data, "shuttle")?
        .into_iter()
        .filter_map(as_int)
        .filter(|m| (0..=MAX_ETA_SECONDS / 60).contains(m))
        .map(|m| now + Span::seconds(m * 60 + 30))
        .collect())
}

/// Pulls `<hash>` out of `"screenservices/<path>", "<hash>"` in an OutSystems script.
fn api_version_in(js: &str, path: &str) -> Option<String> {
    let needle = format!("\"{}\"", path.trim_start_matches('/'));
    let rest = js[js.find(&needle)? + needle.len()..].trim_start().strip_prefix(',')?.trim_start().strip_prefix('"')?;
    let hash = &rest[..rest.find('"')?];
    (!hash.is_empty() && hash.len() < 64).then(|| hash.to_string())
}

fn parse_route_stops(data: &Value) -> Vec<(String, String)> {
    let list = data["BusStopList"]["List"].as_array().map(Vec::as_slice).unwrap_or_default();
    list.iter()
        .filter_map(|s| Some((s["BusStopCode"].as_str()?.to_string(), s["BusStopLongName"].as_str()?.to_string())))
        .collect()
}

/// NTU gives public bus ETAs in seconds from `now`.
fn parse_ntu_public(data: &Value, now: DateTime<FixedOffset>) -> Result<Vec<DateTime<FixedOffset>>> {
    if data["IsETA_NotFound"].as_bool() == Some(true) {
        return Ok(vec![]);
    }
    Ok(ntu_etas(data, "public bus")?
        .into_iter()
        .filter_map(as_int)
        .filter(|s| (-59..=MAX_ETA_SECONDS).contains(s))
        .map(|s| now + Span::seconds(s))
        .collect())
}

pub struct LtaDataMall {
    agent: ureq::Agent,
    account_key: String,
}

/// One stop's BusArrival response.
pub struct LtaStop {
    body: Value,
    /// How far this board's clock is ahead of LTA's, when that's more than `MAX_SKEW`
    skew: Span,
}

impl LtaStop {
    /// A difference this small is the request's travel time and the `Date` header's whole
    /// seconds, not a wrong clock.
    const MAX_SKEW: i64 = 30;

    fn new(body: Value, server_time: Option<DateTime<FixedOffset>>, now: DateTime<FixedOffset>) -> Self {
        let skew = server_time.map(|t| now - t).filter(|s| s.num_seconds().abs() > Self::MAX_SKEW);
        Self { body, skew: skew.unwrap_or_else(Span::zero) }
    }

    /// One service's arrival times at `now`, soonest first. LTA sends clock times, so they are
    /// moved by the difference between this board's clock and LTA's: a board whose clock is
    /// wrong still counts down the right number of minutes.
    pub fn arrivals(&self, service: &str, now: DateTime<FixedOffset>) -> Result<Vec<DateTime<FixedOffset>>> {
        Ok(arrivals(&self.body, service, now - self.skew)?.into_iter().map(|t| t + self.skew).collect())
    }
}

impl LtaDataMall {
    const URL: &'static str = "https://datamall2.mytransport.sg/ltaodataservice/v3/BusArrival";

    pub fn new(account_key: String) -> Self {
        Self { agent: agent(), account_key }
    }

    /// Every service's next buses at one stop, in one request; pick a service out with
    /// `LtaStop::arrivals`.
    pub fn stop(&self, stop: &str) -> Result<LtaStop> {
        let mut resp = self
            .agent
            .get(Self::URL)
            .query("BusStopCode", stop)
            .header("AccountKey", &self.account_key)
            .header("accept", "application/json")
            .call()?;
        let server_time = resp
            .headers()
            .get("date")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| DateTime::parse_from_rfc2822(s).ok());
        Ok(LtaStop::new(resp.body_mut().read_json()?, server_time, crate::now()))
    }
}

/// One service's arrival times from a BusArrival response, dropping buses that left more than
/// a minute before `now`. A stop with no buses still lists `Services`, as an empty array.
fn arrivals(resp: &Value, service: &str, now: DateTime<FixedOffset>) -> Result<Vec<DateTime<FixedOffset>>> {
    let services = resp["Services"].as_array().ok_or("LTA: no Services in the response")?;
    let mut out: Vec<_> = services
        .iter()
        .filter(|s| s["ServiceNo"].as_str() == Some(service))
        .flat_map(|s| ["NextBus", "NextBus2", "NextBus3"].map(|k| s[k]["EstimatedArrival"].clone()))
        .filter_map(|v| DateTime::parse_from_rfc3339(v.as_str()?).ok()) // "" when LTA has no estimate
        .filter(|t| (*t - now).num_seconds() > -60)
        .collect();
    out.sort();
    Ok(out)
}

/// Where one row's arrival times come from.
pub enum Source {
    /// NTU campus shuttle at an NTU stop name
    Shuttle { stop: String, route: String },
    /// Public bus at a 5-digit LTA stop code
    Public { stop: String, service: String },
}

/// Both data sources together: NTU Omnibus, and LTA DataMall when there is an AccountKey.
pub struct Sources {
    ntu: NtuOmnibus,
    lta: Option<LtaDataMall>,
}

impl Sources {
    pub fn new(lta_account_key: Option<String>) -> Self {
        Self { ntu: NtuOmnibus::new(), lta: lta_account_key.map(LtaDataMall::new) }
    }

    pub fn uses_lta(&self) -> bool {
        self.lta.is_some()
    }

    /// Arrival times for each of `sources`, in the same order.
    pub fn fetch(&mut self, sources: &[Source]) -> Vec<Result<Vec<DateTime<FixedOffset>>>> {
        // LTA answers for every service at a stop at once, so rows sharing a stop share a request
        let mut lta_stops: HashMap<&str, std::result::Result<LtaStop, String>> = HashMap::new();
        sources
            .iter()
            .map(|source| match (source, &self.lta) {
                (Source::Shuttle { stop, route }, _) => self.ntu.shuttle(stop, route),
                (Source::Public { stop, service }, None) => self.ntu.public(stop, service),
                (Source::Public { stop, service }, Some(lta)) => {
                    let resp = lta_stops.entry(stop).or_insert_with(|| lta.stop(stop).map_err(|e| e.to_string()));
                    let from_lta = match resp {
                        Ok(resp) => resp.arrivals(service, crate::now()),
                        Err(e) => Err(e.clone().into()),
                    };
                    // While LTA is failing, NTU's relay can stand in for the services it carries.
                    // It answers "no buses" for any other service, so only its buses are believed
                    from_lta.or_else(|e| match self.ntu.public(stop, service) {
                        Ok(arrivals) if !arrivals.is_empty() => Ok(arrivals),
                        _ => Err(e),
                    })
                }
            })
            .collect()
    }

    /// Prints what each source answers right now, for finding out what changed when a row
    /// stops working: NTU's version hashes (built in, and in its live scripts) and the raw
    /// response behind each of `sources`.
    pub fn probe(&mut self, sources: &[Source]) {
        let show = |what: String, result: Result<String>| match result {
            Ok(v) => println!("{what}\n    {v}"),
            Err(e) => println!("{what}\n    ERROR: {e}"),
        };
        show("NTU moduleVersion".into(), self.ntu.module_version(true));
        for (path, built_in) in [NtuOmnibus::FMS, NtuOmnibus::LTA, NtuOmnibus::ROUTE_STOPS] {
            let live = self.ntu.find_api_version(path);
            let note = match &live {
                Ok(live) if live == built_in => " (same as built in)",
                Ok(_) => " (NTU redeployed; the board looks this up by itself)",
                Err(_) => "",
            };
            show(format!("NTU apiVersion {path}\n    built in: {built_in}"), live.map(|v| format!("live:     {v}{note}")));
        }
        for source in sources {
            match source {
                Source::Shuttle { stop, route } => show(
                    format!("NTU shuttle {route} at {stop}"),
                    self.ntu.post(NtuOmnibus::FMS, json!({"BusStopCode": stop, "RouteCode": route})).map(|d| d.to_string()),
                ),
                Source::Public { stop, service } => {
                    show(
                        format!("NTU relay {service} at {stop}"),
                        self.ntu.post(NtuOmnibus::LTA, json!({"BusStopCode": stop, "ServiceNo": service})).map(|d| d.to_string()),
                    );
                    if let Some(lta) = &self.lta {
                        show(
                            format!("LTA DataMall stop {stop} (all services)"),
                            lta.stop(stop).map(|s| format!("board clock ahead of LTA by {} s\n    {}", s.skew.num_seconds(), s.body)),
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hits NTU's live server: cargo test -- --ignored recovers_from_stale_api_version
    #[test]
    #[ignore]
    fn recovers_from_stale_api_version() {
        let mut ntu = NtuOmnibus::new();
        let path = NtuOmnibus::FMS.0;
        ntu.api_versions.insert(path.into(), "stalestalestalestale00".into());
        ntu.shuttle("Hall 11 Blk 55", "Red").unwrap();
        assert_eq!(ntu.api_versions[path], NtuOmnibus::FMS.1);
    }

    #[test]
    fn finds_api_version_in_script() {
        let js = r#"return controller.callServerAction("GetETAAndNextETA_FMS", "screenservices/CampusShuttle_MUI/ActionGetETAAndNextETA_FMS", "2vqnARteBS8UFZvhYCydcw", inputs"#;
        let path = "/screenservices/CampusShuttle_MUI/ActionGetETAAndNextETA_FMS";
        assert_eq!(api_version_in(js, path).as_deref(), Some("2vqnARteBS8UFZvhYCydcw"));
        assert_eq!(api_version_in(js, "/screenservices/CampusShuttle_MUI/ActionGone"), None);
        // minified: no space after the comma
        assert_eq!(api_version_in(&js.replace("\", \"", "\",\""), path).as_deref(), Some("2vqnARteBS8UFZvhYCydcw"));
    }

    /// Response shapes NTU's live server sent on 2026-11-13.
    #[test]
    fn exception_is_an_error_not_empty_data() {
        let path = NtuOmnibus::FMS.0;
        let ok = json!({"versionInfo": {}, "data": {"ETA": "4", "NextETA": "10"}, "rolesInfo": ","});
        assert_eq!(response_data(path, &ok).unwrap(), json!({"ETA": "4", "NextETA": "10"}));
        let wrong_view = json!({"versionInfo": {}, "data": {}, "rolesInfo": ",", "exception":
            {"name": "ServerException", "specificType": "System.InvalidOperationException", "message": "No role validation found"}});
        assert!(response_data(path, &wrong_view).unwrap_err().to_string().contains("No role validation found"));
        assert!(response_data(path, &json!({"versionInfo": {}})).is_err());
    }

    #[test]
    fn route_stops_keep_order_and_codes() {
        let data = json!({"BusStopList": {"List": [
            {"BusStopCode": "Hall 11 Blk 55", "BusStopLongName": "Hall 11 Blk 55"},
            {"BusStopCode": "22009", "BusStopLongName": "Boon Lay Int"},
        ]}});
        assert_eq!(
            parse_route_stops(&data),
            [("Hall 11 Blk 55".into(), "Hall 11 Blk 55".into()), ("22009".into(), "Boon Lay Int".into())]
        );
        assert!(parse_route_stops(&json!({})).is_empty());
    }

    fn now() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-11-05T17:30:00+08:00").unwrap()
    }

    fn at(t: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(&format!("2026-11-05T{t}+08:00")).unwrap()
    }

    #[test]
    fn shuttle_minutes_become_times_and_dash_dropped() {
        // "2" is 2 to 3 minutes away: mid-minute, so it still reads 2 just before the next fetch
        assert_eq!(parse_ntu_shuttle(&json!({"ETA": "2", "NextETA": "-"}), now()).unwrap(), vec![at("17:32:30")]);
        assert!(parse_ntu_shuttle(&json!({"ETA": "-", "NextETA": "-"}), now()).unwrap().is_empty());
        // a stop that isn't on the route
        assert!(parse_ntu_shuttle(&json!({"ETA": "", "NextETA": ""}), now()).unwrap().is_empty());
    }

    #[test]
    fn changed_or_absurd_ntu_responses() {
        // an exception's empty data, or renamed fields, is an error rather than "no bus"
        assert!(parse_ntu_shuttle(&json!({}), now()).is_err());
        assert!(parse_ntu_shuttle(&json!({"Eta": "2", "NextEta": "5"}), now()).is_err());
        assert!(parse_ntu_public(&json!({}), now()).is_err());
        // numbers too large to be ETAs are dropped instead of overflowing the date
        let huge = json!({"ETA": "9223372036854775807", "NextETA": "3"});
        assert_eq!(parse_ntu_shuttle(&huge, now()).unwrap(), vec![at("17:33:30")]);
        assert_eq!(parse_ntu_public(&huge, now()).unwrap(), vec![at("17:30:03")]);
    }

    #[test]
    fn ntu_public() {
        let data = json!({"ETA": "242", "NextETA": "334", "IsETA_NotFound": false});
        assert_eq!(parse_ntu_public(&data, now()).unwrap(), vec![at("17:34:02"), at("17:35:34")]);
        assert!(parse_ntu_public(&json!({"ETA": "0", "NextETA": "0", "IsETA_NotFound": true}), now()).unwrap().is_empty());
    }

    #[test]
    fn lta_v3() {
        let resp = json!({"Services": [
            {"ServiceNo": "179", "NextBus": {"EstimatedArrival": "2026-11-05T17:31:00+08:00"}},
            {"ServiceNo": "199",
             "NextBus": {"EstimatedArrival": "2026-11-05T17:34:02+08:00"},
             "NextBus2": {"EstimatedArrival": "2026-11-05T17:41:00+08:00"},
             "NextBus3": {"EstimatedArrival": ""}}]});
        assert_eq!(arrivals(&resp, "199", now()).unwrap(), vec![at("17:34:02"), at("17:41:00")]);
        assert_eq!(arrivals(&resp, "179", now()).unwrap(), vec![at("17:31:00")]);
        assert!(arrivals(&resp, "179B", now()).unwrap().is_empty());
        assert!(arrivals(&json!({"Services": []}), "199", now()).unwrap().is_empty());
        assert!(arrivals(&json!({"fault": "denied"}), "199", now()).is_err());
    }

    #[test]
    fn lta_times_follow_the_boards_clock() {
        let body = || json!({"Services": [{"ServiceNo": "199", "NextBus": {"EstimatedArrival": "2026-11-05T17:34:00+08:00"}}]});
        // LTA says 17:30 and the bus comes at 17:34. A board that thinks it's 17:40 shows the
        // bus at 17:44, still four minutes away, instead of dropping it as long gone
        let fast = LtaStop::new(body(), Some(now()), at("17:40:00"));
        assert_eq!(fast.arrivals("199", at("17:40:00")).unwrap(), vec![at("17:44:00")]);
        // a few seconds apart, or no Date header: times are used as sent
        let close = LtaStop::new(body(), Some(now()), at("17:30:04"));
        assert_eq!(close.arrivals("199", at("17:30:04")).unwrap(), vec![at("17:34:00")]);
        assert_eq!(LtaStop::new(body(), None, now()).arrivals("199", now()).unwrap(), vec![at("17:34:00")]);
    }
}
