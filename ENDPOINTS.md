# NTU Omnibus (`sg.edu.ntu.apps.ntuomnibus`) Live API Endpoints

The **NTU Omnibus** mobile app is built on the OutSystems platform and communicates with its live production backend hosted at:

- **Base URL (Production):** `https://apps.ntu.edu.sg/NTUOmnibus` (CNAME to `ntu.outsystemsenterprise.com`)
- **Other Environments (from APK `pinning.json`):**
  - `https://apps-dev.ntu.edu.sg/NTUOmnibus`
  - `https://apps-uat.ntu.edu.sg/NTUOmnibus`
  - `https://apps-preprod.ntu.edu.sg/NTUOmnibus`

Every endpoint below was called against the live server on 2026-11-13 with the `apiVersion` and `viewName` listed, and returned data; each `apiVersion` also matched the one in NTU's live web scripts.

All data requests are OutSystems `screenservices` JSON `POST` requests (plus `GET` endpoints for module version & manifest discovery). None of the bus tracking or arrival endpoints require user login—only an anonymous OutSystems CSRF header and cookie pair.

---

## 1. Authentication, Headers & `viewName` Rules

Include the following headers and cookies on all `POST` requests:

```http
Content-Type: application/json
X-CSRFToken: T6C+9iB49TLra4jEsMeSckDMNhQ=
Cookie: nr1Users=lid=Anonymous; nr2Users=crf=T6C%2b9iB49TLra4jEsMeSckDMNhQ%3d
```

Every `POST` body must include:
1. `versionInfo.moduleVersion`: current deployment token from `/moduleservices/moduleversioninfo` (currently `"A9V6_wjfsjF2reMXeyPbiQ"`)
2. `versionInfo.apiVersion`: the action-specific hash listed below for each endpoint
3. `viewName`: **must match the parent OutSystems screen that hosts the block/action**, otherwise the server returns `"No role validation found"`:
   - Map & single-stop endpoints use `"MainFlow.Shuttle"`
   - All-stops / search bulk endpoints (`BusStop.mvc.js`) use `"CommonFlow.AllRoutesServiceDetails"`
   - Route service detail bulk endpoints (`WB_ServiceDetail.mvc.js`) use `"CommonFlow.SelectedRouteServiceDetail"`

---

## 2. Module & Manifest Discovery (`GET`)

### 2.1 Get Current `moduleVersion` Token
- **Method:** `GET`
- **URL:** `https://apps.ntu.edu.sg/NTUOmnibus/moduleservices/moduleversioninfo`

```bash
curl -s "https://apps.ntu.edu.sg/NTUOmnibus/moduleservices/moduleversioninfo"
# {"versionToken":"A9V6_wjfsjF2reMXeyPbiQ"}
```

### 2.2 Get Full Application Manifest & Asset Map
- **Method:** `GET`
- **URL:** `https://apps.ntu.edu.sg/NTUOmnibus/moduleservices/moduleinfo`

---

## 3. Live Bus Tracking & Arrival Endpoints (`POST`)

### Summary Table

| # | Feature | Endpoint Path (appended to Base URL) | `apiVersion` | Required `viewName` |
| :- | :--- | :--- | :--- | :--- |
| **3.1** | **Bulk: All Stops (FMS + SBS) Metadata & FMS ETAs** | `/screenservices/CampusShuttle_MUI/MainFlow/BusStop/DataActionGetBusStop` | `ppAJEP2ygsuaWrt1pbM4wA` | `CommonFlow.AllRoutesServiceDetails` |
| **3.2** | **Bulk: All Campus Shuttle Stop ETAs (FMS)** | `/screenservices/NTUMobile_MCW/ActionGetFMSBusStopETA` | `7+sPcG48iaiQFj4lnWvUpw` | `CommonFlow.SelectedRouteServiceDetail` |
| **3.3** | **Bulk: All LTA Stop Arrivals Across All Services (179/179A/199)** | `/screenservices/CampusShuttle_MUI/MainFlow/BusStop/ScreenDataSetGetLTABusArrivals` | `LDM5MpYQLgaD1PVLxbbLTA` | `CommonFlow.AllRoutesServiceDetails` |
| **3.4** | **Bulk: Single Route Ordered Stops + ETAs (FMS or SBS)** | `/screenservices/NTUMobile_MCW/NTU_Blocks/WB_ServiceDetail/DataActionGetBusServiceDetails` | `1PUVaxmcJyMdguP9kAtsOw` | `CommonFlow.SelectedRouteServiceDetail` |
| **3.5** | **Bulk: Single LTA Service Stop Arrivals (`179`/`179A`/`199`)** | `/screenservices/NTUMobile_MCW/NTU_Blocks/WB_ServiceDetail/ScreenDataSetGetLTABusArrivalsByServiceNo` | `BqrM_9lMY87FkCfA_axUaw` | `CommonFlow.SelectedRouteServiceDetail` |
| **3.6** | **Live Active Bus GPS, Speed, Heading & Crowd** | `/screenservices/CampusShuttle_MUI/MainFlow/RenderMap/DataActionGetActiveBusServicesData` | `X18_TOlljR63ZC0gtWKItg` | `MainFlow.Shuttle` |
| **3.7** | **Single Stop Campus Shuttle ETA (FMS, in mins)** | `/screenservices/CampusShuttle_MUI/ActionGetETAAndNextETA_FMS` | `2vqnARteBS8UFZvhYCydcw` | `MainFlow.Shuttle` |
| **3.8** | **Single Stop Public Bus ETA (LTA, in secs)** | `/screenservices/CampusShuttle_MUI/ActionGetETAAndNextETA_LTA` | `NBbTcPPSIJ15dc3NGRGWtA` | `MainFlow.Shuttle` |
| **3.9** | **Bus Stops (`PickupPoints`) & Route Polyline (`CheckPoint`)** | `/screenservices/CampusShuttle_MUI/MainFlow/RenderMap/DataActionGetPickupCheckPoints` | `T2ld7BA3eca74ndBz+xz1Q` | `MainFlow.Shuttle` |
| **3.10** | **Available Routes List** | `/screenservices/CampusShuttle_MUI/MainFlow/RenderMap/DataActionGetRoutesList` | `LTmSrAcALxEQn_uoCNK2RA` | `MainFlow.Shuttle` |
| **3.11** | **Combined LTA & FMS Routes List** | `/screenservices/CampusShuttle_MUI/MainFlow/ServiceRoutes/ActionGetLTAandFMSBusRoutes` | `htdh8CpQakdHm6LGTVX8eQ` | `MainFlow.Shuttle` |
| **3.12** | **Campus Shuttle Routes & Stop Sequence Metadata** | `/screenservices/CampusShuttle_MUI/ActionSyncFMS_BusRoutess` | `rvFBkDtRqR4nWDochiHLLA` | `MainFlow.Shuttle` |
| **3.13** | **Campus Shuttle Operating Hours (First/Last Bus)** | `/screenservices/CampusShuttle_MUI/ActionSyncFMS_RouteMinMaxTimes` | `EHlcrnJw4NQ7LmhC4_6ooQ` | `MainFlow.Shuttle` |
| **3.14** | **Public Bus (179/179A/199) Stop Sequence & First/Last Bus** | `/screenservices/CampusShuttle_MUI/ActionSyncLTA_BusRoutess` | `+qvFBJxaYmoWFMHlM6vOSQ` | `MainFlow.Shuttle` |
| **3.15** | **Route Hex Color Codes & Display Order** | `/screenservices/CampusShuttle_MUI/MainFlow/RenderMap/ActionGetRoutesCodejson` | `iGX_QZtcG4abQY7HMQwidg` | `MainFlow.Shuttle` |
| **3.16** | **Polling Intervals & Map Config (`GetTokenAndKeys`)** | `/screenservices/CampusShuttle_MUI/MainFlow/RenderMap/ActionGetTokenAndKeys` | `XUq8iwa+7Auzmu8ihWzX3w` | `MainFlow.Shuttle` |
| **3.17** | **Service Announcements** | `/screenservices/NTUOmnibus/ActionGetAnnoucements` | `agAUaSp+Gt+ysBfwCD5SRA` | `MainFlow.Shuttle` |

---

### 3.1 Bulk: All Bus Stops (FMS + SBS) with Names, Route Colors & FMS ETAs
Extracted from the APK's `BusStop.mvc.js` (`CommonFlow.AllRoutesServiceDetails`). Returns every campus shuttle stop-route pair and every SBS (`179`/`179A`/`199`) stop-route pair (116 in total when this was written)—including human-readable `BusStopLongName`, `BusStopCode`, `RouteId`, `ColorCode`, and FMS `ArrivalTime`/`NextArrivalTime`—in a single request.

```bash
curl -s -X POST "https://apps.ntu.edu.sg/NTUOmnibus/screenservices/CampusShuttle_MUI/MainFlow/BusStop/DataActionGetBusStop" \
  -H "Content-Type: application/json" \
  -H "X-CSRFToken: T6C+9iB49TLra4jEsMeSckDMNhQ=" \
  -b "nr1Users=lid=Anonymous; nr2Users=crf=T6C%2b9iB49TLra4jEsMeSckDMNhQ%3d" \
  -d '{
    "versionInfo": {
      "moduleVersion": "A9V6_wjfsjF2reMXeyPbiQ",
      "apiVersion": "ppAJEP2ygsuaWrt1pbM4wA"
    },
    "viewName": "CommonFlow.AllRoutesServiceDetails",
    "screenData": {
      "variables": {
        "Search": ""
      }
    },
    "clientVariables": {}
  }'
```

---

### 3.2 Bulk: All Campus Shuttle Stop ETAs (`ActionGetFMSBusStopETA`)
Returns `ArrivalTime` and `NextArrivalTime` for every campus shuttle stop and route across NTU in one request.

```bash
curl -s -X POST "https://apps.ntu.edu.sg/NTUOmnibus/screenservices/NTUMobile_MCW/ActionGetFMSBusStopETA" \
  -H "Content-Type: application/json" \
  -H "X-CSRFToken: T6C+9iB49TLra4jEsMeSckDMNhQ=" \
  -b "nr1Users=lid=Anonymous; nr2Users=crf=T6C%2b9iB49TLra4jEsMeSckDMNhQ%3d" \
  -d '{
    "versionInfo": {
      "moduleVersion": "A9V6_wjfsjF2reMXeyPbiQ",
      "apiVersion": "7+sPcG48iaiQFj4lnWvUpw"
    },
    "viewName": "CommonFlow.SelectedRouteServiceDetail",
    "inputParameters": {
      "ServiceId": "NTU"
    }
  }'
```

---

### 3.3 Bulk: All LTA Bus Stop Arrivals Across All Services (`ScreenDataSetGetLTABusArrivals`)
Returns every current LTA arrival row (`ServiceNo`, `BusStop`, `NextBusInSeconds`, `NextBus2InSeconds`) across `179`, `179A`, and `199` in one request.

```bash
curl -s -X POST "https://apps.ntu.edu.sg/NTUOmnibus/screenservices/CampusShuttle_MUI/MainFlow/BusStop/ScreenDataSetGetLTABusArrivals" \
  -H "Content-Type: application/json" \
  -H "X-CSRFToken: T6C+9iB49TLra4jEsMeSckDMNhQ=" \
  -b "nr1Users=lid=Anonymous; nr2Users=crf=T6C%2b9iB49TLra4jEsMeSckDMNhQ%3d" \
  -d '{
    "versionInfo": {
      "moduleVersion": "A9V6_wjfsjF2reMXeyPbiQ",
      "apiVersion": "LDM5MpYQLgaD1PVLxbbLTA"
    },
    "viewName": "CommonFlow.AllRoutesServiceDetails",
    "screenData": {
      "variables": {
        "Search": ""
      }
    },
    "clientVariables": {},
    "inputParameters": {
      "startIndex": 0,
      "maxRecords": 200
    }
  }'
```

**Response (abbreviated):**
```json
{
  "data": {
    "List": {
      "List": [
        {
          "LTA_BusArrival": {
            "ServiceNo": "179",
            "BusStop": "22521",
            "NextBusInSeconds": -16,
            "NextBus2InSeconds": 257
          }
        }
      ]
    }
  }
}
```

---

### 3.4 Bulk: Single Route Ordered Stops + ETAs (`DataActionGetBusServiceDetails`)
Returns the ordered stop list (`BusStopLongName`, `BusStopCode`, `ArrivalTime`, `NextArrivalTime`) for a specific route (`Red`, `Blue`, `Green`, `Brown`, `Grey`, `179`, `179A`, `199`).

```bash
curl -s -X POST "https://apps.ntu.edu.sg/NTUOmnibus/screenservices/NTUMobile_MCW/NTU_Blocks/WB_ServiceDetail/DataActionGetBusServiceDetails" \
  -H "Content-Type: application/json" \
  -H "X-CSRFToken: T6C+9iB49TLra4jEsMeSckDMNhQ=" \
  -b "nr1Users=lid=Anonymous; nr2Users=crf=T6C%2b9iB49TLra4jEsMeSckDMNhQ%3d" \
  -d '{
    "versionInfo": {
      "moduleVersion": "A9V6_wjfsjF2reMXeyPbiQ",
      "apiVersion": "1PUVaxmcJyMdguP9kAtsOw"
    },
    "viewName": "CommonFlow.SelectedRouteServiceDetail",
    "screenData": {
      "variables": {
        "BusRouteId": "179",
        "RouteName": "179",
        "IsSBS": true,
        "Route": "179"
      }
    },
    "clientVariables": {}
  }'
```

---

### 3.5 Bulk: Single LTA Service Stop Arrivals (`ScreenDataSetGetLTABusArrivalsByServiceNo`)
Returns `NextBusInSeconds` and `NextBus2InSeconds` for all stops on a single LTA route (`179`, `179A`, or `199`).

```bash
curl -s -X POST "https://apps.ntu.edu.sg/NTUOmnibus/screenservices/NTUMobile_MCW/NTU_Blocks/WB_ServiceDetail/ScreenDataSetGetLTABusArrivalsByServiceNo" \
  -H "Content-Type: application/json" \
  -H "X-CSRFToken: T6C+9iB49TLra4jEsMeSckDMNhQ=" \
  -b "nr1Users=lid=Anonymous; nr2Users=crf=T6C%2b9iB49TLra4jEsMeSckDMNhQ%3d" \
  -d '{
    "versionInfo": {
      "moduleVersion": "A9V6_wjfsjF2reMXeyPbiQ",
      "apiVersion": "BqrM_9lMY87FkCfA_axUaw"
    },
    "viewName": "CommonFlow.SelectedRouteServiceDetail",
    "screenData": {
      "variables": {
        "BusRouteId": "179",
        "RouteName": "179",
        "IsSBS": true,
        "Route": "179"
      }
    },
    "clientVariables": {},
    "inputParameters": {
      "startIndex": 0,
      "maxRecords": 100
    }
  }'
```

---

### 3.6 Live Active Bus GPS, Speed, Heading & Crowd Level
Returns real-time vehicle positions (`Lat`, `Lng`), license plates (`Vehplate`), speed (`Speed`), heading (`Direction`), and passenger load (`CrowdLevel`) for a given route.

```bash
curl -s -X POST "https://apps.ntu.edu.sg/NTUOmnibus/screenservices/CampusShuttle_MUI/MainFlow/RenderMap/DataActionGetActiveBusServicesData" \
  -H "Content-Type: application/json" \
  -H "X-CSRFToken: T6C+9iB49TLra4jEsMeSckDMNhQ=" \
  -b "nr1Users=lid=Anonymous; nr2Users=crf=T6C%2b9iB49TLra4jEsMeSckDMNhQ%3d" \
  -d '{
    "versionInfo": {
      "moduleVersion": "A9V6_wjfsjF2reMXeyPbiQ",
      "apiVersion": "X18_TOlljR63ZC0gtWKItg"
    },
    "viewName": "MainFlow.Shuttle",
    "screenData": {
      "variables": {
        "RouteId": "Red",
        "IsSBS_Route": false
      }
    },
    "clientVariables": {}
  }'
```

---

### 3.7 Single Stop Campus Shuttle Arrival ETA (FMS, in minutes)

```bash
curl -s -X POST "https://apps.ntu.edu.sg/NTUOmnibus/screenservices/CampusShuttle_MUI/ActionGetETAAndNextETA_FMS" \
  -H "Content-Type: application/json" \
  -H "X-CSRFToken: T6C+9iB49TLra4jEsMeSckDMNhQ=" \
  -b "nr1Users=lid=Anonymous; nr2Users=crf=T6C%2b9iB49TLra4jEsMeSckDMNhQ%3d" \
  -d '{
    "versionInfo": {
      "moduleVersion": "A9V6_wjfsjF2reMXeyPbiQ",
      "apiVersion": "2vqnARteBS8UFZvhYCydcw"
    },
    "viewName": "MainFlow.Shuttle",
    "inputParameters": {
      "BusStopCode": "Hall 12",
      "RouteCode": "Red"
    }
  }'
```

---

### 3.8 Single Stop Public Bus Arrival ETA (LTA, in seconds)

```bash
curl -s -X POST "https://apps.ntu.edu.sg/NTUOmnibus/screenservices/CampusShuttle_MUI/ActionGetETAAndNextETA_LTA" \
  -H "Content-Type: application/json" \
  -H "X-CSRFToken: T6C+9iB49TLra4jEsMeSckDMNhQ=" \
  -b "nr1Users=lid=Anonymous; nr2Users=crf=T6C%2b9iB49TLra4jEsMeSckDMNhQ%3d" \
  -d '{
    "versionInfo": {
      "moduleVersion": "A9V6_wjfsjF2reMXeyPbiQ",
      "apiVersion": "NBbTcPPSIJ15dc3NGRGWtA"
    },
    "viewName": "MainFlow.Shuttle",
    "inputParameters": {
      "BusStopCode": "27211",
      "ServiceNo": "179"
    }
  }'
```

---

### 3.9 Bus Stops (`PickupPoints`) & Route Polyline Coordinates (`CheckPoint`)

```bash
curl -s -X POST "https://apps.ntu.edu.sg/NTUOmnibus/screenservices/CampusShuttle_MUI/MainFlow/RenderMap/DataActionGetPickupCheckPoints" \
  -H "Content-Type: application/json" \
  -H "X-CSRFToken: T6C+9iB49TLra4jEsMeSckDMNhQ=" \
  -b "nr1Users=lid=Anonymous; nr2Users=crf=T6C%2b9iB49TLra4jEsMeSckDMNhQ%3d" \
  -d '{
    "versionInfo": {
      "moduleVersion": "A9V6_wjfsjF2reMXeyPbiQ",
      "apiVersion": "T2ld7BA3eca74ndBz+xz1Q"
    },
    "viewName": "MainFlow.Shuttle",
    "screenData": {
      "variables": {
        "RouteId": "Red",
        "IsSBS_Route": false
      }
    },
    "clientVariables": {}
  }'
```

---

### Response shapes to expect

Every response below came back with HTTP 200 on 2026-11-13, so the status code says nothing; read the body.

| Case | Response |
| :--- | :--- |
| 3.7, buses coming | `"data":{"ETA":"4","NextETA":"10"}` (whole minutes, as strings) |
| 3.7, stop is on the route but no bus is estimated | `"data":{"ETA":"-","NextETA":"-"}` |
| 3.7, unknown stop or route, or a stop that isn't on that route | `"data":{"ETA":"","NextETA":""}` |
| 3.8, bus coming | `"data":{"ETA":"512","IsETA_NotFound":false,"NextETA":"1451"}` (seconds; can be negative for a bus that just left) |
| Stale `apiVersion` | `"versionInfo":{"hasModuleVersionChanged":false,"hasApiVersionChanged":true},"data":{}` |
| Wrong `viewName` | `"data":{}` plus `"exception":{"name":"ServerException","specificType":"System.InvalidOperationException","message":"No role validation found"}` |

Not yet checked: whether a route that isn't running that day (Green at a weekend, Brown on a weekday) answers `"-"` or `""`.

Each action's current `apiVersion` is in the module's `<Module>.controller.js`, written as
`"screenservices/<path>", "<apiVersion>"`. `ntu-bus --probe` prints the built-in and live values and the raw
response behind each configured row.

---

## 4. Other Endpoints & Integrations Found in the APK

| Feature | Endpoint / Resource | `apiVersion` / Value | `viewName` |
| :--- | :--- | :--- | :--- |
| **NTU Indoor/Campus Map (MazeMap)** | `/screenservices/NTUMobile_MCW/MainFlow/Maps/DataActionGetIndoorMapData` | `UZa0+u5JahoghJ1Ie7kkuw` → returns `https://maps.ntu.edu.sg?cb=...` | `MainFlow.NTUMaps` |
| **Google Maps JS API Key** | `/screenservices/CampusShuttle_MUI/MainFlow/RenderMap/ActionGetGoogleAPIKey` | `IhcObRvFU_y+mzzvZQY3ag` | `MainFlow.Shuttle` |
| **Announcements List** | `/screenservices/NTUMobile_MCW/NTU_Blocks/WB_Announcements/DataActionGetAnnouncementList2` | `Yk4v+wktyptypPR4HqNSZg` | `CommonFlow.Annoucements` |
| **Check Holiday / Special Day** | `/screenservices/NTUMobile_MCW/ActionCheckSGPublicHoliday`<br>`/screenservices/NTUMobile_MCW/ActionCheckSGSpecialDay` | `fscpDj_Fk99IsWsp5BcNnA`<br>`m4IdRYmMB2fT8nBHOQsuRw` | `CommonFlow.SelectedRouteServiceDetail` |
| **External App Deep Links** | `/screenservices/NTUOmnibus/MainFlow/UsefulApps/DataActionGetExternalAppLink` | `UPqRaro72bZmswXQ9TQdsw` (`ntupass://login`) | `MainFlow.UsefulApps` |
| **Lyon Chatbot** | Embedded WebView URL in `NTUMobile_MCW.MainFlow.LyonChat.mvc.js` | `https://ntu-general.firebaseapp.com/` | — |
| **Campus Security Incident Form** | Embedded WebView URL in `NTUMobile_MCW.MainFlow.SecurityIncidentForm.mvc.js` | `https://www.cognitoforms.com/Ntu18/SecurityIncidentForm` | — |
| **Digital Business Card Feedback** | Embedded WebView URL in `NTUMobile_MCW.MainFlow.DigitalBusinessCard_feedbackForm.mvc.js` | `https://www.cognitoforms.com/f/JG2FBc8akEuv8tQRA-c4Kw/383` | — |

---

## 5. How ETA is Calculated (FMS vs. LTA)

### 5.1 NTU Campus Shuttles (`Red`, `Blue`, `Green`, `Brown`, `Grey`) — FMS Pipeline
1. **Fleet management system (`CampusShuttle_MIS` module):**
   - The shuttles are run by ComfortDelGro Bus, whose buses carry GPS telematics, and the NTU Omnibus app is jointly run by ComfortDelGro Bus and NTU.
   - Its data model tracks each active vehicle's trip (`jobid`), plate (`plate`) and route checkpoint index (`px`), with an `_etas` list per stop holding `eta_s` (seconds) and `eta` (minutes).
   - The app refreshes live bus positions on the map every **3 seconds** for campus shuttles (`FMSActiveBusfrequency: 3000` from `GetTokenAndKeys`).
2. **OutSystems Backend:**
   - Exposes the first two approaching buses as `ETA` (`ArrivalTime`) and `NextETA` (`NextArrivalTime`) in **integer minutes**.
3. **Client Display Logic (`getETA_FMS$Action` in `CampusShuttle_MUI.controller.js`):**
   - `ETA === ""` or `int(ETA) < 0` → `"-"`
   - `ETA === "0"` → `"<1min"`
   - `int(ETA) === 1` → `"1min"`
   - `int(ETA) > 1` → `"{ETA}mins"` (e.g. `"2mins"`)

### 5.2 Public SBS Buses (`179`, `179A`, `199`) — LTA DataMall Pipeline
1. **NTU's copy of LTA data (`LTA_BusArrival`):**
   - NTU's backend returns `NextBusInSeconds` and `NextBus2InSeconds` from its own `LTA_BusArrival` records rather than the app calling LTA DataMall directly.
   - The app refreshes public bus positions on the map every **30 seconds** (`LTAActiveBusfrequency: 30000` from `GetTokenAndKeys`).
2. **Client Calculation & Display Logic (`getETA_LTA$Action` in `CampusShuttle_MUI.controller.js`):**
   - Converts raw seconds to integer minutes by truncating division by `60` (`nextBusInMin = trunc(ETA_seconds / 60)`):
     - `IsETA_NotFound === true` → `"-"`
     - `-59 < int(ETA_seconds) < 59` → `"ARR"`
     - `nextBusInMin < 0` → `"-"`
     - `nextBusInMin === 0` → `"<1min"`
     - `nextBusInMin === 1` → `"1min"`
     - `nextBusInMin > 1` → `"{nextBusInMin}mins"`
