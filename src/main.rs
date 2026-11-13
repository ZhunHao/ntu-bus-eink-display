//! Bus arrival board for a Raspberry Pi Zero 2 W + Waveshare 2.13" Touch e-Paper HAT.
//!
//! ntu-bus [--config config.toml] [--preview board.png] [--once]
//! ntu-bus --stops ROUTE    lists a route's stops, e.g. --stops Red or --stops 199
//! ntu-bus [--config config.toml] --probe    prints what NTU and LTA answer right now

mod render;
mod sources;
#[cfg(target_os = "linux")]
mod hw;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use chrono::{DateTime, FixedOffset, Utc};
use serde::Deserialize;

use render::{Framebuffer, Kind, Row};
use sources::{NtuOmnibus, Source, Sources};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    title: String,
    #[serde(default = "default_refresh")]
    refresh_seconds: u64,
    #[serde(default = "default_full_every")]
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))] // only the e-paper uses it
    full_refresh_every: u32,
    #[serde(default)]
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))] // only the e-paper uses it
    flip: bool,
    rows: Vec<RowConfig>,
}
fn default_refresh() -> u64 {
    30
}
fn default_full_every() -> u32 {
    20
}

/// One `[[rows]]` entry. Unknown keys are rejected, so a typo like `hide_when_emtpy` is an
/// error instead of being silently ignored.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RowConfig {
    kind: RowKind,
    label: String,
    /// Shown under the route badge when there's room (two rows or fewer)
    #[serde(default)]
    dest: Option<String>,
    stop: String,
    #[serde(default)]
    route: Option<String>,
    #[serde(default)]
    service: Option<String>,
    #[serde(default)]
    hide_when_empty: bool,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum RowKind {
    Shuttle,
    Public,
}

impl RowConfig {
    /// Shuttle rows need `route`, public rows need `service`, and neither takes the other's.
    fn source(&self) -> Result<Source> {
        let stop = self.stop.clone();
        match (self.kind, &self.route, &self.service) {
            (RowKind::Shuttle, Some(route), None) => Ok(Source::Shuttle { stop, route: route.clone() }),
            (RowKind::Public, None, Some(service)) => Ok(Source::Public { stop, service: service.clone() }),
            (RowKind::Shuttle, _, _) => Err(format!("row {}: a shuttle row needs `route` and no `service`", self.label).into()),
            (RowKind::Public, _, _) => Err(format!("row {}: a public row needs `service` and no `route`", self.label).into()),
        }
    }
}

/// What the board knows about one row between fetches.
#[derive(Default)]
struct Known {
    /// From the last successful fetch; `None` until one succeeds
    arrivals: Option<Vec<DateTime<FixedOffset>>>,
    /// Fetches that have failed since then
    missed: u32,
    /// The latest failure, so a failure that repeats every fetch is logged once
    error: Option<String>,
}

impl Known {
    /// Takes in a fetch's result. A failure leaves the previous data in place and counts the miss.
    fn record(&mut self, label: &str, result: Result<Vec<DateTime<FixedOffset>>>) {
        match result {
            Ok(arrivals) => {
                if self.error.is_some() {
                    eprintln!("{label}: working again");
                }
                *self = Known { arrivals: Some(arrivals), missed: 0, error: None };
            }
            Err(e) => {
                let e = e.to_string();
                if self.error.as_ref() != Some(&e) {
                    eprintln!("{label}: {e}");
                }
                self.missed += 1;
                self.error = Some(e);
            }
        }
    }

    /// Usable arrival times. One missed fetch keeps the last ones, so a single dropped request
    /// doesn't blank the row; a second means the board has nothing it can stand behind.
    fn usable(&self) -> Option<&Vec<DateTime<FixedOffset>>> {
        self.arrivals.as_ref().filter(|_| self.missed <= 1)
    }
}

/// The rows to draw at `now`, from what's known. Recomputed on every redraw, so a tap can
/// redraw straight away and buses that have left drop off without a new fetch.
fn rows_to_show(rows: &[RowConfig], known: &[Known], now: DateTime<FixedOffset>) -> Vec<Row> {
    rows.iter()
        .zip(known)
        .filter_map(|(cfg, k)| {
            let usable = k.usable();
            let row = Row {
                label: cfg.label.clone(),
                dest: cfg.dest.clone(),
                kind: match cfg.kind {
                    RowKind::Shuttle => Kind::Shuttle,
                    RowKind::Public => Kind::Public,
                },
                arrivals: usable.cloned().unwrap_or_default(),
                error: usable.is_none(),
            };
            shown(cfg.hide_when_empty, row.error, row.upcoming(now).is_empty()).then_some(row)
        })
        .collect()
}

/// Routes that don't always run (Green on weekends, Brown on weekdays, the exam-period Grey
/// loop) drop off the board when they have no buses, giving the other rows more room. A row
/// with no data keeps its place, so it shows "no data" instead of vanishing.
fn shown(hide_when_empty: bool, error: bool, no_buses: bool) -> bool {
    !(hide_when_empty && !error && no_buses)
}

/// Current time in Singapore
pub fn now() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap())
}

/// What the main loop waits for.
enum Event {
    /// The screen was tapped
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))] // only the e-paper has touch
    Tap,
    /// A fetch finished: one result per row
    Fetched(Vec<Result<Vec<DateTime<FixedOffset>>>>),
}

/// Where frames go: the e-paper HAT on the Pi, or a PNG for previewing anywhere.
trait Screen {
    fn show(&mut self, fb: &Framebuffer) -> Result<()>;
    fn sleep(&mut self) -> Result<()>;
}

struct Preview(String);

impl Screen for Preview {
    fn show(&mut self, fb: &Framebuffer) -> Result<()> {
        fb.write_png(&self.0, 3)?;
        eprintln!("wrote {}", self.0);
        Ok(())
    }
    fn sleep(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(target_os = "linux")]
struct EPaper {
    epd: hw::Epd,
    flip: bool,
    /// Partial refreshes allowed between full (flashing) ones, which clear ghosting
    full_every: u32,
    /// Partial refreshes since the last full one
    partials: u32,
    /// The image on the panel, once a full refresh has drawn one. `None` at the start and after
    /// any failure, which makes the next draw a full one
    shown: Option<Vec<u8>>,
}

#[cfg(target_os = "linux")]
impl EPaper {
    /// Sets up the panel, and the touch panel to send `Event::Tap` on every tap.
    fn new(flip: bool, full_every: u32, events: mpsc::Sender<Event>) -> Result<Self> {
        let gpio = rppal::gpio::Gpio::new()?;
        let mut epd = hw::Epd::new(&gpio)?;
        epd.clear()?;
        hw::spawn_touch(&gpio, move || events.send(Event::Tap).is_ok())?;
        Ok(Self { epd, flip, full_every, partials: 0, shown: None })
    }
}

#[cfg(target_os = "linux")]
impl Screen for EPaper {
    fn show(&mut self, fb: &Framebuffer) -> Result<()> {
        let buf = fb.to_epd(self.flip);
        let result = match &self.shown {
            // Nothing changed (often the case within a minute): leave the panel alone
            Some(shown) if *shown == buf => return Ok(()),
            Some(_) if self.partials < self.full_every => {
                self.partials += 1;
                self.epd.display_partial(&buf)
            }
            _ => {
                self.partials = 0;
                self.epd.display_base(&buf)
            }
        };
        // A partial refresh only redraws what changed since the base image, so after any
        // failure (the panel may have reset) the next draw has to be a full one
        self.shown = result.is_ok().then_some(buf);
        result
    }
    fn sleep(&mut self) -> Result<()> {
        self.epd.sleep()
    }
}

/// Prints a route's stops in order, with the value to use as `stop` in config.toml.
fn list_stops(route: &str) -> Result<()> {
    let stops = NtuOmnibus::new().route_stops(route)?;
    if stops.is_empty() {
        return Err(format!("no stops found for {route}; NTU knows Red, Blue, Green, Brown, Grey, 179, 179A and 199").into());
    }
    for (code, name) in stops {
        if code == name {
            println!("{code}");
        } else {
            println!("{code}  {name}");
        }
    }
    Ok(())
}

/// Reads the config: the board's settings, and where each row's times come from.
fn load(toml: &str) -> Result<(Config, Vec<Source>)> {
    let cfg: Config = toml::from_str(toml)?;
    let sources = cfg.rows.iter().map(RowConfig::source).collect::<Result<_>>()?;
    Ok((cfg, sources))
}

fn run() -> Result<()> {
    let mut config_path = "config.toml".to_string();
    let (mut preview, mut once, mut probe) = (None, false, false);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--config" => config_path = args.next().ok_or("--config needs a path")?,
            "--preview" => preview = Some(args.next().ok_or("--preview needs a PNG path")?),
            "--once" => once = true,
            "--probe" => probe = true,
            "--stops" => return list_stops(&args.next().ok_or("--stops needs a route, e.g. Red or 199")?),
            _ => {
                return Err(format!(
                    "unknown argument {a}\nusage: ntu-bus [--config FILE] [--preview PNG] [--once]\n       ntu-bus --stops ROUTE\n       ntu-bus [--config FILE] --probe"
                )
                .into())
            }
        }
    }
    let (cfg, sources) = load(&std::fs::read_to_string(&config_path)?).map_err(|e| format!("{config_path}: {e}"))?;

    let mut fetcher = Sources::new(std::env::var("LTA_ACCOUNT_KEY").ok().filter(|k| !k.is_empty()));
    if probe {
        fetcher.probe(&sources);
        return Ok(());
    }
    eprintln!(
        "public buses from {}",
        if fetcher.uses_lta() { "LTA DataMall" } else { "NTU Omnibus relay (set LTA_ACCOUNT_KEY to use LTA)" }
    );

    let (events_tx, events) = mpsc::channel();
    let mut screen: Box<dyn Screen> = match preview {
        Some(path) => Box::new(Preview(path)),
        #[cfg(target_os = "linux")]
        None => Box::new(EPaper::new(cfg.flip, cfg.full_refresh_every, events_tx.clone())?),
        #[cfg(not(target_os = "linux"))]
        None => return Err("the e-paper HAT only works on the Pi; use --preview board.png here".into()),
    };

    // Fetching has its own thread, so a slow or dead network never keeps a tap waiting
    let refresh = Duration::from_secs(cfg.refresh_seconds);
    thread::spawn(move || loop {
        if events_tx.send(Event::Fetched(fetcher.fetch(&sources))).is_err() {
            return;
        }
        thread::sleep(refresh);
    });

    let mut known: Vec<Known> = cfg.rows.iter().map(|_| Known::default()).collect();
    let mut fetched = false;
    let mut clock = false; // a tap toggles "minutes away" <-> "arrival clock time"
    let mut screen_ok = true;
    // Redraw on every fetch, and straight away from the same data on every tap
    loop {
        match events.recv().map_err(|_| "the fetch thread stopped")? {
            Event::Tap => clock = !clock,
            Event::Fetched(results) => {
                for ((row, k), result) in cfg.rows.iter().zip(&mut known).zip(results) {
                    k.record(&row.label, result);
                }
                fetched = true;
            }
        }
        if !fetched {
            continue; // tapped before the first fetch came back: nothing to draw yet
        }
        let at = now();
        let fb = render::render(&rows_to_show(&cfg.rows, &known, at), &cfg.title, at, clock);
        // Log only when the screen stops or starts responding, not on every refresh
        match screen.show(&fb) {
            Err(e) if screen_ok => {
                eprintln!("display: {e}");
                screen_ok = false;
            }
            Ok(()) if !screen_ok => {
                eprintln!("display: responding again");
                screen_ok = true;
            }
            _ => {}
        }
        if once {
            return screen.sleep(); // e-paper keeps the last image without power
        }
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: i64) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-11-05T17:30:00+08:00").unwrap() + chrono::Duration::seconds(secs)
    }

    fn config(toml: &str) -> Result<Vec<RowConfig>> {
        load(toml).map(|(cfg, _)| cfg.rows)
    }

    const TWO_ROWS: &str = r#"
        title = "Hall 11"
        [[rows]]
        kind = "shuttle"
        label = "Grey"
        stop = "Hall 11 Blk 55"
        route = "Grey"
        hide_when_empty = true
        [[rows]]
        kind = "public"
        label = "199"
        stop = "27199"
        service = "199"
    "#;

    #[test]
    fn hide_when_empty_only_hides_routes_with_no_buses() {
        assert!(!shown(true, false, true)); // route not running: hidden
        assert!(shown(true, true, true)); // no data: stays, shows "no data"
        assert!(shown(true, false, false)); // buses coming: shown
        assert!(shown(false, false, true)); // not marked: always shown
    }

    #[test]
    fn config_typos_and_mixed_up_fields_are_errors() {
        assert!(config(TWO_ROWS).is_ok());
        assert!(config(&TWO_ROWS.replace("hide_when_empty", "hide_when_emtpy")).is_err());
        assert!(config(&TWO_ROWS.replace("route = \"Grey\"", "service = \"Grey\"")).is_err());
        assert!(config(&TWO_ROWS.replace("service = \"199\"", "route = \"199\"")).is_err());
        assert!(config(&TWO_ROWS.replace("title", "titel")).is_err());
    }

    #[test]
    fn last_good_data_survives_one_missed_fetch() {
        let rows = config(TWO_ROWS).unwrap();
        // Both rows fetched fine, then failed `missed` times
        let known = |missed| {
            let mut known = [Known::default(), Known::default()];
            known[0].record("Grey", Ok(vec![])); // Grey not running
            known[1].record("199", Ok(vec![at(300)]));
            for k in &mut known {
                (0..missed).for_each(|_| k.record("row", Err("timed out".into())));
            }
            known
        };
        // Grey is hidden; 199 shows its bus, also after one failed fetch
        for missed in [0, 1] {
            let shown = rows_to_show(&rows, &known(missed), at(30));
            assert_eq!(shown.len(), 1);
            assert_eq!((shown[0].label.as_str(), shown[0].error), ("199", false));
        }
        // a second failure: both rows say "no data", so Grey comes back too
        let shown = rows_to_show(&rows, &known(2), at(60));
        assert_eq!(shown.len(), 2);
        assert!(shown.iter().all(|r| r.error));
        // nothing fetched yet
        assert!(rows_to_show(&rows, &[Known::default(), Known::default()], at(0)).iter().all(|r| r.error));
        // and a fetch that works again brings the times straight back
        let mut recovered = known(2);
        recovered[1].record("199", Ok(vec![at(300)]));
        assert!(!rows_to_show(&rows, &recovered, at(60))[1].error);
    }
}
