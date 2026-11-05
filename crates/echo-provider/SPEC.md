# Echo Desktop: Provider & Cipher Extraction Architecture Specification

**Document Version:** 1.0.0  
**Target Module:** `crates/echo-provider`  
**Status:** Canonical Design & Architecture Specification  
**License:** GPL-3.0  

---

## 1. Executive Summary & System Topology

Echo Desktop is designed as a hybrid, audiophile-grade desktop music player. Its primary architecture is decoupled into two complementary provider layers:

1. **YouTube Music InnerTube Engine (Primary Catalog & Universal Fallback):**  
   Provides unlimited, global access to millions of songs, albums, artists, contextual radio mixes, and synchronized lyrics via reverse-engineered Google InnerTube API endpoints (`WEB_REMIX` client). It incorporates client signature deobfuscation (`s`-parameter unscrambling and `n`-parameter transform evaluation) inspired by open-source systems such as **BravePipe**, **NewPipe**, and **RustyPipe** to defeat YouTube CDN bandwidth throttling.

2. **Echo Lossless & High-Bitrate Extensions (`lossless-extension` & `320kbs-extension`):**  
   Autonomous microservice providers exposing standardized REST contracts. They resolve studio-quality lossless streams (FLAC 24-bit/192kHz or 16-bit/44.1kHz) and 320kbps MP3/AAC streams when available, using ISRC (International Standard Recording Code) and fuzzy metadata correlation.

```
                              +---------------------------------------+
                              |         Echo Desktop Audio Core        |
                              |   (Track Request & Playback Pipeline) |
                              +-------------------+-------------------+
                                                  |
                                                  v
                              +---------------------------------------+
                              |      Dynamic Fallback Router          |
                              |      (Fidelity Tier Management)       |
                              +--------+---------------------+--------+
                                       |                     |
                  Tier 1 / Tier 2      |                     | Tier 3 / Tier 4
             (Lossless / 320k Extensions)                    | (YouTube Music InnerTube)
                                       v                     v
              +-------------------------------+    +-------------------------------+
              |   EchoExtensionProvider       |    |  YouTubeMusicProvider         |
              |   - GET /v1/health            |    |  - POST /youtubei/v1/search   |
              |   - GET /v1/search            |    |  - POST /youtubei/v1/next     |
              |   - GET /v1/stream            |    |  - POST /youtubei/v1/browse   |
              +---------------+---------------+    |  - POST /youtubei/v1/player   |
                              |                    +---------------+---------------+
                              |                                    |
                              v                                    v
              +-------------------------------+    +-------------------------------+
              | Local/Remote Daemon Microserv |    | Player Cipher & Deobfuscator  |
              | - FLAC 24-bit/192kHz (Tidal/  |    | - Base.js AST Pattern Parser  |
              |   Qobuz / Deezer mirrors)     |    | - s-descrambler (swap/splice) |
              | - MP3 320kbps CBR streams     |    | - n-parameter de-throttling   |
              +-------------------------------+    +---------------+---------------+
                                                                   |
                                                                   v
                                                   +-------------------------------+
                                                   | Googlevideo CDN Stream URL    |
                                                   | - Opus 160kbps (itag 251)     |
                                                   | - AAC 128kbps (itag 140)      |
                                                   +-------------------------------+
```

---

## 2. YouTube Music InnerTube Reverse Engineering & Endpoint Architecture

Google InnerTube is YouTube's unified backend API service (`/youtubei/v1/*`). Echo Desktop communicates with InnerTube using the `WEB_REMIX` client profile, which mimics the official YouTube Music web client and eliminates the need for user OAuth tokens for standard streaming.

### 2.1 Canonical Header Matrix & Client Identity

All HTTP requests to `https://music.youtube.com/youtubei/v1/` must include the following headers and context payload:

```http
POST /youtubei/v1/{endpoint} HTTP/1.1
Host: music.youtube.com
User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36
Accept: */*
Accept-Language: en-US,en;q=0.9
Content-Type: application/json
Origin: https://music.youtube.com
Referer: https://music.youtube.com/
X-YouTube-Client-Name: 67
X-YouTube-Client-Version: 1.20240401.01.00
X-Goog-Visitor-Id: <cached_visitor_data>
Sec-Fetch-Mode: cors
Sec-Fetch-Site: same-origin
```

#### Base InnerTube Context JSON Payload
Every endpoint request body wraps the operational payload inside an InnerTube `context`:

```json
{
  "context": {
    "client": {
      "clientName": "WEB_REMIX",
      "clientVersion": "1.20240401.01.00",
      "hl": "en",
      "gl": "US",
      "experimentIds": [],
      "utcOffsetMinutes": 0
    },
    "user": {
      "lockedSafetyMode": false
    },
    "request": {
      "useSsl": true
    }
  }
}
```

---

### 2.2 Search Endpoint (`/youtubei/v1/search`)

* **Endpoint URL:** `https://music.youtube.com/youtubei/v1/search`
* **HTTP Method:** `POST`

#### Request Payload
To restrict results strictly to songs, albums, or artists, pre-defined base64-encoded protobuf parameter tokens are provided:

```json
{
  "context": {
    "client": {
      "clientName": "WEB_REMIX",
      "clientVersion": "1.20240401.01.00",
      "hl": "en",
      "gl": "US"
    }
  },
  "query": "Hans Zimmer Interstellar",
  "params": "EgWKAQIIAWoKEAkQBRAKEAMQBBAF"
}
```

##### Pre-defined Filter `params` Mapping:
* **Songs only:** `EgWKAQIIAWoKEAkQBRAKEAMQBBAF`
* **Videos only:** `EgWKAQIQAWoKEAkQBRAKEAMQBBAF`
* **Albums only:** `EgWKAQIBAWoKEAkQBRAKEAMQBBAF`
* **Artists only:** `EgWKAQIgAWoKEAkQBRAKEAMQBBAF`
* **Playlists only:** `EgWKAQIoAWoKEAkQBRAKEAMQBBAF`

#### Response Parsing & Extraction Pipeline
The response contains deeply nested renderers. Echo Desktop navigates the hierarchy:

```
response
 └── contents
      └── tabbedSearchResultsRenderer
           └── tabs[0]
                └── tabRenderer
                     └── content
                          └── sectionListRenderer
                               └── contents[]
                                    └── musicShelfRenderer
                                         └── contents[]
                                              └── musicResponsiveListItemRenderer
```

##### Field Extraction Strategy:
1. **Video ID (`id`):**  
   `flexColumns[0].musicResponsiveListItemFlexColumnRenderer.text.runs[0].navigationEndpoint.watchEndpoint.videoId`
2. **Title (`title`):**  
   `flexColumns[0].musicResponsiveListItemFlexColumnRenderer.text.runs[0].text`
3. **Artists (`artist`):**  
   Iterate `flexColumns[1].musicResponsiveListItemFlexColumnRenderer.text.runs`, filter out separators (` • `), extract runs with `browseEndpoint.browseId` prefixed with `UC`.
4. **Album (`album`):**  
   Run with `browseEndpoint.browseId` prefixed with `MPREb_` or matching album navigation.
5. **Duration (`duration_seconds`):**  
   The final run of `flexColumns[1]` or `fixedColumns[0]` contains the timestamp (e.g., `"4:05"` or `"1:02:30"`). Parsed via:
   $$\text{seconds} = \sum_{i=0}^{n} \text{part}_i \times 60^{n - 1 - i}$$
6. **Thumbnails (`thumbnail_url`):**  
   `thumbnail.musicThumbnailRenderer.thumbnail.thumbnails[]` -> Select highest resolution entry (typically $544 \times 544$ or resize using `=w1080-h1080-l90-rj` query parameter).

---

### 2.3 Next / Radio Auto-Queue Endpoint (`/youtubei/v1/next`)

Used to establish infinite playback queues ("Song Radio") seeded from an initial track ID.

* **Endpoint URL:** `https://music.youtube.com/youtubei/v1/next`
* **HTTP Method:** `POST`

#### Request Payload
To initiate contextual auto-radio for track `videoId`:

```json
{
  "context": {
    "client": {
      "clientName": "WEB_REMIX",
      "clientVersion": "1.20240401.01.00",
      "hl": "en",
      "gl": "US"
    }
  },
  "videoId": "UDVtMYq8loc",
  "playlistId": "RDAMVMUDVtMYq8loc",
  "isAudioOnly": true
}
```
*(Prefixing `RDAMVM` before the `videoId` instructs InnerTube's recommendation engine to dynamically synthesize a contextual infinite radio station).*

#### Continuation Requests (Infinite Scrolling)
When the queue approaches exhaustion, the continuation token extracted from the previous response is dispatched:

```json
{
  "context": { /* standard client */ },
  "continuation": "4qmFsgK6ARIYUkRBT...continuation_token"
}
```

#### Response Extraction:
* Initial Queue Path:  
  `contents.singleColumnMusicWatchNextResultsRenderer.tabbedRenderer.watchNextTabbedResultsRenderer.tabs[0].tabRenderer.content.musicQueueRenderer.content.playlistPanelRenderer.contents[]` -> `playlistPanelVideoRenderer`.
* Continuation Token Path:  
  `continuations[0].nextRadioContinuationData.continuation` or `continuations[0].invalidationContinuationData.continuation`.

---

### 2.4 Browse Endpoint (`/youtubei/v1/browse`)

Retrieves structured entity metadata for Artists, Albums, and Curated Playlists.

* **Endpoint URL:** `https://music.youtube.com/youtubei/v1/browse`
* **HTTP Method:** `POST`

#### Request Payload:
```json
{
  "context": {
    "client": {
      "clientName": "WEB_REMIX",
      "clientVersion": "1.20240401.01.00",
      "hl": "en",
      "gl": "US"
    }
  },
  "browseId": "MPREb_0nQcE5p9Z2x"
}
```

#### Entity Browse ID Conventions:
* **Artist:** `UC...` or `FEmusic_artist_...` (Yields top songs, albums carousel, singles, related artists).
* **Album:** `MPREb_...` (Yields album title, artist, year, release date, track count, full tracklist with track numbering).
* **Explore / Home:** `FEmusic_home`, `FEmusic_explore`, `FEmusic_charts`.

---

### 2.5 Player Endpoint (`/youtubei/v1/player`)

The authoritative endpoint for extracting stream manifests, itags, and signature cipher wrappers.

* **Endpoint URL:** `https://music.youtube.com/youtubei/v1/player`
* **HTTP Method:** `POST`

#### Request Payload:
```json
{
  "context": {
    "client": {
      "clientName": "WEB_REMIX",
      "clientVersion": "1.20240401.01.00",
      "hl": "en",
      "gl": "US"
    }
  },
  "videoId": "UDVtMYq8loc",
  "playbackContext": {
    "contentPlaybackContext": {
      "signatureTimestamp": 19842
    }
  }
}
```
> **Critical:** The `signatureTimestamp` (`sts`) is extracted directly from YouTube's current active JavaScript player script (`base.js`). Omitting it or passing a stale timestamp triggers `403 Forbidden` responses from Google Video edge clusters.

#### Audio Stream Format Specifications (`streamingData.adaptiveFormats`):

| itag | Container / Audio Codec | Bitrate | Sample Rate | Audio Channels | Primary Preference Tier |
|:---:|:---:|:---:|:---:|:---:|:---:|
| **251** | WebM / **Opus** | ~160 kbps | 48.0 kHz | 2 | **Tier 3 (Primary InnerTube)** |
| **140** | MP4 / **AAC (LC)** | ~128 kbps | 44.1 kHz | 2 | **Tier 4 (InnerTube Fallback)** |
| **250** | WebM / **Opus** | ~70 kbps | 48.0 kHz | 2 | Tier 5 (Bandwidth Saver) |
| **249** | WebM / **Opus** | ~50 kbps | 48.0 kHz | 2 | Tier 6 (Extreme Bandwidth Saver) |

---

## 3. JavaScript Player Signature Cipher Deobfuscation Algorithms

YouTube protects its GoogleVideo CDN delivery endpoints using two distinct layers of obfuscation:

1. **Signature Cipher (`s` or `sig` parameter):** Protects the URL against unauthorized construction by scrambling the verification signature token.
2. **Bandwidth Throttling (`n` parameter transform):** Prevents automated bulk downloading. If the `n` parameter present in the query string is not transformed according to the player's JavaScript arithmetic function, the CDN throttles TCP packet transmission to $\le 50\text{ kbps}$, causing fatal audio buffer underruns.

```
                              [ YouTube Player base.js ]
                                          |
                +-------------------------+-------------------------+
                |                                                   |
                v                                                   v
      [ s-Parameter Descrambler ]                         [ n-Parameter Descrambler ]
    Extract: s=<scrambled_sig>                          Extract: &n=<token>
                |                                                   |
      Parse AST Transformations:                          Parse Dynamic AST / Function:
      1. a.reverse()                                      1. Array splitting
      2. a.splice(0, b)                                   2. Table shift / modulus mapping
      3. a[0] = a[b % len]                                3. String reconstruction
                |                                                   |
                v                                                   v
     Plaintext Signature (&sig=...)                      De-throttled Token (&n=...)
                \                                                   /
                 \                                                 /
                  v                                               v
                 +-------------------------------------------------+
                 |  Final Authorizedgooglevideo.com Audio Stream   |
                 |      Full Speed Buffering & Playback Ready      |
                 +-------------------------------------------------+
```

### 3.1 Player JavaScript Acquisition & Lifecycle Cache

1. Echo Desktop queries `https://music.youtube.com` or `https://www.youtube.com/iframe_api`.
2. Locates the player base URL via regex:
   ```regex
   (?:/s/player/|player_ias\.vflset/)([a-zA-Z0-9_-]+)(?:/base\.js|/player_ias\.vflset)
   ```
3. Canonical base.js URL resolved:
   ```
   https://music.youtube.com/s/player/{player_id}/player_ias.vflset/en_US/base.js
   ```
4. **Signature Timestamp (`sts`) Extraction:**
   Matches:
   ```regex
   signatureTimestamp\s*:\s*(\d+)
   ```
   Extracted integer is saved in the player cache and sent with every `/player` InnerTube call.

---

### 3.2 `s`-Parameter Descrambling Algorithm (Inspired by NewPipe & RustyPipe)

When a format object in `adaptiveFormats` contains `signatureCipher` instead of `url`:
```
signatureCipher = "s=aBc123...&sp=sig&url=https%3A%2F%2Frr1---sn-xxx.googlevideo.com%2Fvideoplayback%3F..."
```

#### Step 1: Identifying the Entrypoint Descramble Function
In `base.js`, YouTube defines the main descrambling function with patterns matching:
```regex
\b[cs]\s*&&\s*[adf]\.set\([^,]+\s*,\s*encodeURIComponent\s*\(\s*([a-zA-Z0-9$]+)\(
```
or
```regex
([a-zA-Z0-9$]{2,3})\s*=\s*function\([a-zA-Z0-9$]+\)\{[a-zA-Z0-9$]+=[a-zA-Z0-9$]+\.split\(""\);
```

Example matched JavaScript:
```javascript
var kx = function(a) {
    a = a.split("");
    AB.ro(a, 2);
    AB.xh(a, 31);
    AB.ro(a, 1);
    AB.mn(a, 3);
    AB.xh(a, 18);
    return a.join("");
};
```

#### Step 2: Locating the Operation Dictionary
The helper object `AB` contains the atomic operations:
```javascript
var AB = {
    ro: function(a) {
        a.reverse()
    },
    xh: function(a, b) {
        var c = a[0];
        a[0] = a[b % a.length];
        a[b % a.length] = c;
    },
    mn: function(a, b) {
        a.splice(0, b);
    }
};
```

#### Step 3: Pure Rust Instruction Engine
Echo Desktop translates these into a strongly-typed intermediate representation:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CipherOp {
    Reverse,
    Swap(usize),
    Splice(usize),
}

pub fn execute_cipher_ops(scrambled: &str, ops: &[CipherOp]) -> String {
    let mut chars: Vec<char> = scrambled.chars().collect();
    let len = chars.len();

    for op in ops {
        match *op {
            CipherOp::Reverse => chars.reverse(),
            CipherOp::Swap(pos) => {
                if len > 0 {
                    chars.swap(0, pos % len);
                }
            }
            CipherOp::Splice(count) => {
                if count < chars.len() {
                    chars.drain(0..count);
                } else {
                    chars.clear();
                }
            }
        }
    }

    chars.into_iter().collect()
}
```

---

### 3.3 `n`-Parameter Throttling Transform Descrambler

YouTube dynamically transforms the `n` parameter query string. If passed unmodified, download speed is throttled to 50 kbps.

#### Step 1: Identifying the `n`-Transform Function Entrypoint
Echo Desktop scans `base.js` for the dispatcher call:
```regex
(?:\.get\("n"\)\)&&\(b=([a-zA-Z0-9$]+)(?:\[(\d+)\])?\([a-zA-Z0-9$]+\)\)
```
or
```regex
var\s+b=a\.get\("n"\);\s*if\(b\)\{\s*b=([a-zA-Z0-9$]+)(?:\[(\d+)\])?\(b\);
```

#### Step 2: Structure of the Obfuscated `n`-Function
The resolved function performs:
1. Array initialization from the input token: `a = a.split("")`.
2. A cryptographic permutation table generated using seeded modular arithmetic:
   ```javascript
   var c = [12, 45, 2, ...]; // pseudo-random displacement lookup
   ```
3. A sequence of bitwise XOR, character code additions, and index rotations.
4. Joining the resulting array into the verified de-throttled token.

#### Step 3: Dual-Mode Execution Architecture
To guarantee 100% operational resilience across YouTube script updates, Echo Desktop implements a **Tiered Deobfuscator**:

1. **Fast Path (Native Bytecode AST Interpreter):**  
   Parses the localized AST into native Rust math/string transforms (executes in $< 100\ \mu\text{s}$, zero heap allocation overhead).
2. **Resilience Fallback (Embedded Sandboxed Micro-V8 / QuickJS):**  
   If YouTube modifies the AST grammar beyond regex capabilities, the isolated `n` function code snippet is evaluated inside an embedded, sandboxed JS runtime (`rquickjs` or `boa_engine`) with strict 50ms execution timeout limits.

---

## 4. Echo Music `lossless-extension` & `320kbs-extension` HTTP REST Contract

The `lossless-extension` (delivering up to FLAC 24-bit/192kHz) and `320kbs-extension` (delivering 320kbps MP3/AAC) communicate with Echo Desktop over local or remote REST microservices.

### 4.1 Topology & Port Conventions
* **`lossless-extension` Default Base URL:** `http://127.0.0.1:45300/v1`
* **`320kbs-extension` Default Base URL:** `http://127.0.0.1:45301/v1`
* **Transport:** HTTP/1.1 or HTTP/2, JSON payloads, seekable stream URLs supporting `Range: bytes=start-end`.

---

### 4.2 Endpoint: Health & Discovery (`GET /v1/health`)

Verifies service availability, supported audio containers, sample rates, and bit depths.

#### Request:
```http
GET /v1/health HTTP/1.1
Host: 127.0.0.1:45300
Accept: application/json
```

#### Response (`200 OK`):
```json
{
  "status": "healthy",
  "extension": {
    "name": "echo-flac-lossless-extension",
    "version": "1.4.2",
    "tier": "lossless_master",
    "capabilities": {
      "formats": ["flac", "alac", "wav"],
      "maxBitDepth": 24,
      "maxSampleRate": 192000,
      "supportsIsrcLookup": true,
      "supportsDirectStreaming": true
    }
  }
}
```

---

### 4.3 Endpoint: Search (`GET /v1/search`)

Searches the extension catalog for tracks matching text queries.

#### Request:
```http
GET /v1/search?q=Daft+Punk+Get+Lucky&type=track&limit=5 HTTP/1.1
Host: 127.0.0.1:45300
Accept: application/json
```

#### Query Parameters:
* `q` *(string, required)*: Search query string.
* `type` *(string, optional)*: `track` (default), `album`, `artist`.
* `limit` *(integer, optional)*: Number of items to return (1-50, default 10).

#### Response (`200 OK`):
```json
{
  "results": [
    {
      "id": "ext_dp_getlucky_flac",
      "isrc": "USQX91300105",
      "title": "Get Lucky (feat. Pharrell Williams & Nile Rodgers)",
      "artist": "Daft Punk",
      "album": "Random Access Memories",
      "duration": 369,
      "releaseYear": 2013,
      "thumbnail": "https://img.provider.net/covers/ram_1400.jpg",
      "availableQualities": [
        {
          "format": "flac",
          "bitDepth": 24,
          "sampleRate": 88200,
          "bitrate": 2822,
          "isLossless": true
        },
        {
          "format": "flac",
          "bitDepth": 16,
          "sampleRate": 44100,
          "bitrate": 1411,
          "isLossless": true
        }
      ]
    }
  ]
}
```

---

### 4.4 Endpoint: Stream Resolution (`GET /v1/stream`)

Resolves a playable, seekable direct audio URL for a given track identifier or metadata set.

#### Request:
```http
GET /v1/stream?id=ext_dp_getlucky_flac&isrc=USQX91300105&quality=lossless_max HTTP/1.1
Host: 127.0.0.1:45300
Accept: application/json
```

#### Query Parameters:
* `id` *(string, required)*: Extension track identifier or YouTube videoId.
* `isrc` *(string, optional)*: International Standard Recording Code for exact correlation.
* `title` *(string, optional)*: Track title for fallback search correlation.
* `artist` *(string, optional)*: Artist name.
* `quality` *(string, optional)*: `lossless_max` (default), `lossless_cd`, `high_320`.

#### Response (`200 OK`):
```json
{
  "url": "https://cdn.lossless.echomusic.internal/streams/dp_getlucky_24_88200.flac?token=exp17412000_sig99281a",
  "format": "flac",
  "bitDepth": 24,
  "sampleRate": 88200,
  "bitrate": 2822,
  "isLossless": true,
  "fileSizeBytes": 128450120,
  "durationSeconds": 369,
  "expiresAt": 1741200000,
  "headers": {
    "User-Agent": "EchoDesktop/1.0",
    "Accept-Ranges": "bytes"
  }
}
```

---

### 4.5 Endpoint: ISRC Direct Resolution (`GET /v1/resolve/isrc/{isrc}`)

Performs unambiguous 1:1 matching using the track's ISRC, bypassing fuzzy text matching.

#### Request:
```http
GET /v1/resolve/isrc/USQX91300105 HTTP/1.1
Host: 127.0.0.1:45300
Accept: application/json
```

#### Response (`200 OK`):
Returns the same response format as `GET /v1/stream`.

---

### 4.6 Standardized Error Schema

```json
{
  "error": {
    "code": "TRACK_NOT_FOUND",
    "message": "No lossless stream matching ISRC USQX91300105 found in extension catalog.",
    "upstreamStatus": 404,
    "retryable": false
  }
}
```

| HTTP Status | Error Code | Description |
|:---:|:---:|:---|
| `400` | `INVALID_PARAMETERS` | Missing required parameters (`id` or `q`). |
| `404` | `TRACK_NOT_FOUND` | Track not found in extension library. |
| `416` | `QUALITY_UNAVAILABLE` | Requested quality tier (`lossless_max`) unavailable for track. |
| `429` | `RATE_LIMITED` | Extension upstream rate limit triggered. |
| `502` | `UPSTREAM_FAILED` | Extension upstream CDN unreachable. |

---

## 5. Concrete Rust Types, Traits & Seamless Fallback Engine

Echo Desktop defines a modular, asynchronous provider architecture with priority-tiered dynamic fallback.

```
       Track Stream Resolution Pipeline
                      │
                      ▼
     ┌──────────────────────────────────┐
     │  Tier 1: Lossless Extension      │ ─── [Success] ──► Return 24-bit/192kHz FLAC
     │  (Exact ISRC / Metadata Match)   │
     └──────────────────────────────────┘
                      │ [Fail / 404]
                      ▼
     ┌──────────────────────────────────┐
     │  Tier 2: 320kbps Extension       │ ─── [Success] ──► Return 320kbps MP3 / AAC
     │  (High-Bitrate CBR Stream)       │
     └──────────────────────────────────┘
                      │ [Fail / 404]
                      ▼
     ┌──────────────────────────────────┐
     │  Tier 3: YouTube Music Opus 251  │ ─── [Success] ──► Return Deciphered Opus ~160kbps
     │  (InnerTube + n/s-descramblers)  │
     └──────────────────────────────────┘
                      │ [Fail / Throttle]
                      ▼
     ┌──────────────────────────────────┐
     │  Tier 4: YouTube Music AAC 140   │ ─── [Success] ──► Return Deciphered AAC ~128kbps
     │  (Universal Baseline Fallback)   │
     └──────────────────────────────────┘
```

### 5.1 Domain Types & Data Models

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// Supported audio encoding codecs across all providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AudioCodec {
    Flac,
    Alac,
    Wav,
    Opus,
    Aac,
    Mp3,
    Unknown,
}

/// Detailed audio fidelity classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AudioFidelity {
    /// Lossy standard stream (e.g. YouTube Music AAC 128kbps)
    LossyStandard = 1,
    /// Lossy high-quality stream (e.g. YouTube Music Opus 160kbps)
    LossyHigh = 2,
    /// Lossy premium stream (e.g. 320kbs-extension MP3/AAC)
    Lossy320 = 3,
    /// Lossless standard CD quality (16-bit / 44.1kHz FLAC)
    LosslessCD = 4,
    /// Lossless Studio Master (24-bit / 48kHz, 96kHz, or 192kHz FLAC)
    LosslessHiRes = 5,
}

/// Unified stream payload with audio properties and authentication headers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioStream {
    /// Playable HTTP/HTTPS URL (Range-compatible)
    pub url: String,
    /// Container and audio codec
    pub codec: AudioCodec,
    /// Bitrate in kilobits per second (e.g. 160, 320, 1411, 4608, 9216)
    pub bitrate_kbps: u32,
    /// Sample rate in Hertz (e.g. 44100, 48000, 96000, 192000)
    pub sample_rate_hz: u32,
    /// Bit depth (16 or 24 for lossless, None for lossy)
    pub bit_depth: Option<u8>,
    /// Quality classification
    pub fidelity: AudioFidelity,
    /// Total file size if advertised by provider
    pub content_length_bytes: Option<u64>,
    /// Custom HTTP headers required to stream this URL
    pub http_headers: HashMap<String, String>,
}

/// Canonical Track entity used across all UI and audio playback layers
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub isrc: Option<String>,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_seconds: u32,
    pub thumbnail_url: Option<String>,
}

/// Errors occurring within provider retrieval pipelines
#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("Network I/O error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Cipher deobfuscation failed: {0}")]
    CipherError(String),

    #[error("Track not found: {0}")]
    TrackNotFound(String),

    #[error("No playable streams found for track: {0}")]
    NoStreamsAvailable(String),

    #[error("API contract error: {0}")]
    ApiContract(String),

    #[error("Rate limited by upstream provider")]
    RateLimited,
}
```

---

### 5.2 Provider Traits Definition

```rust
/// Common provider trait implemented by all catalog and stream sources
#[async_trait]
pub trait MusicProvider: Send + Sync {
    /// Friendly display name of this provider
    fn name(&self) -> &'static str;

    /// Provider capability check
    fn supported_fidelity(&self) -> AudioFidelity;

    /// Search catalog by text query
    async fn search(&self, query: &str) -> Result<Vec<Track>, ProviderError>;

    /// Resolve an audio stream for a track
    async fn resolve_stream(&self, track: &Track) -> Result<AudioStream, ProviderError>;
}
```

---

### 5.3 YouTube Music Cipher & Deobfuscator Engine Implementation

```rust
use regex::Regex;

/// Player script cache holding timestamp and compiled cipher operations
pub struct PlayerCipherState {
    pub signature_timestamp: u32,
    pub cipher_ops: Vec<CipherOp>,
}

pub struct YouTubeCipherEngine {
    client: reqwest::Client,
    cached_state: tokio::sync::RwLock<Option<PlayerCipherState>>,
}

impl YouTubeCipherEngine {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            cached_state: tokio::sync::RwLock::new(None),
        }
    }

    /// Extract cipher operations and timestamp from base.js
    pub async fn fetch_and_parse_player(&self, base_js_url: &str) -> Result<PlayerCipherState, ProviderError> {
        let resp = self.client.get(base_js_url).send().await?.text().await?;
        
        // 1. Extract signatureTimestamp
        let sts_regex = Regex::new(r#"signatureTimestamp\s*:\s*(\d+)"#)
            .map_err(|e| ProviderError::CipherError(e.to_string()))?;
        let sts = sts_regex.captures(&resp)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .unwrap_or(19842);

        // 2. Extract s-parameter descrambler function body
        let fn_entry_regex = Regex::new(r#"\b[cs]\s*&&\s*[adf]\.set\([^,]+\s*,\s*encodeURIComponent\s*\(\s*([a-zA-Z0-9$]+)\("#)
            .map_err(|e| ProviderError::CipherError(e.to_string()))?;
        
        let fn_name = fn_entry_regex.captures(&resp)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .ok_or_else(|| ProviderError::CipherError("Failed to locate signature function name".into()))?;

        // 3. Parse AST transformations (reverse, splice, swap)
        let ops = self.parse_transform_operations(&resp, fn_name)?;

        Ok(PlayerCipherState {
            signature_timestamp: sts,
            cipher_ops: ops,
        })
    }

    fn parse_transform_operations(&self, script: &str, fn_name: &str) -> Result<Vec<CipherOp>, ProviderError> {
        let fn_pattern = format!(r#"{}\s*=\s*function\([a-zA-Z0-9$]+\)\{{([^\}}]+)\}}"#, regex::escape(fn_name));
        let fn_regex = Regex::new(&fn_pattern).map_err(|e| ProviderError::CipherError(e.to_string()))?;
        
        let fn_body = fn_regex.captures(script)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str())
            .ok_or_else(|| ProviderError::CipherError("Failed to parse signature function body".into()))?;

        let mut ops = Vec::new();
        let call_regex = Regex::new(r#"([a-zA-Z0-9$]+)\.([a-zA-Z0-9$]+)\([a-zA-Z0-9$]+(?:,\s*(\d+))?\)"#)
            .map_err(|e| ProviderError::CipherError(e.to_string()))?;

        for cap in call_regex.captures_iter(fn_body) {
            let obj_name = &cap[1];
            let method_name = &cap[2];
            let arg: usize = cap.get(3).map_or(0, |m| m.as_str().parse().unwrap_or(0));

            // Inspect method implementation in helper object
            let method_pattern = format!(
                r#"{}\s*:\s*function\([^)]*\)\{{([^}}]+)\}}"#,
                regex::escape(method_name)
            );
            if let Ok(method_regex) = Regex::new(&method_pattern) {
                if let Some(method_cap) = method_regex.captures(script) {
                    let code = &method_cap[1];
                    if code.contains("reverse") {
                        ops.push(CipherOp::Reverse);
                    } else if code.contains("splice") {
                        ops.push(CipherOp::Splice(arg));
                    } else if code.contains("[0]") || code.contains("%") {
                        ops.push(CipherOp::Swap(arg));
                    }
                }
            }
        }

        Ok(ops)
    }

    /// Deobfuscates signature cipher into verified URL
    pub fn descramble_signature(&self, cipher_str: &str, ops: &[CipherOp]) -> Result<String, ProviderError> {
        let mut params = HashMap::new();
        for pair in cipher_str.split('&') {
            let mut parts = pair.splitn(2, '=');
            if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
                params.insert(k, v);
            }
        }

        let s = params.get("s").ok_or_else(|| ProviderError::CipherError("Missing s parameter".into()))?;
        let sp = params.get("sp").copied().unwrap_or("sig");
        let raw_url = params.get("url").ok_or_else(|| ProviderError::CipherError("Missing url parameter".into()))?;
        let unencoded_url = urlencoding::decode(raw_url).map_err(|e| ProviderError::CipherError(e.to_string()))?;

        let deciphered_sig = crate::execute_cipher_ops(s, ops);
        let separator = if unencoded_url.contains('?') { "&" } else { "?" };
        
        Ok(format!("{}{}{}={}", unencoded_url, separator, sp, deciphered_sig))
    }
}
```

---

### 5.4 Seamless Multi-Tier Dynamic Fallback Router

```rust
/// Multi-tiered dynamic stream fallback orchestrator
pub struct DynamicFallbackRouter {
    tier1_lossless: Option<Arc<dyn MusicProvider>>,
    tier2_highbitrate: Option<Arc<dyn MusicProvider>>,
    tier3_innertube: Arc<dyn MusicProvider>,
}

impl DynamicFallbackRouter {
    pub fn new(
        tier1_lossless: Option<Arc<dyn MusicProvider>>,
        tier2_highbitrate: Option<Arc<dyn MusicProvider>>,
        tier3_innertube: Arc<dyn MusicProvider>,
    ) -> Self {
        Self {
            tier1_lossless,
            tier2_highbitrate,
            tier3_innertube,
        }
    }

    /// Primary entry point: Attempts highest fidelity resolution first,
    /// gracefully cascading down tiers upon failure.
    pub async fn resolve_best_stream(&self, track: &Track) -> Result<AudioStream, ProviderError> {
        // Tier 1: Lossless Extension (FLAC 24-bit/192kHz or 16-bit/44.1kHz)
        if let Some(ref lossless) = self.tier1_lossless {
            tracing::debug!("Attempting Tier 1 [Lossless Extension] for track: {}", track.id);
            match lossless.resolve_stream(track).await {
                Ok(stream) => {
                    tracing::info!(
                        "Tier 1 matched! Codec: {:?}, Rate: {}Hz, Depth: {:?}bit, Bitrate: {}kbps",
                        stream.codec,
                        stream.sample_rate_hz,
                        stream.bit_depth,
                        stream.bitrate_kbps
                    );
                    return Ok(stream);
                }
                Err(err) => {
                    tracing::warn!("Tier 1 resolution skipped: {}. Falling back to Tier 2.", err);
                }
            }
        }

        // Tier 2: 320kbps Extension (MP3 / AAC 320kbps CBR)
        if let Some(ref high_bitrate) = self.tier2_highbitrate {
            tracing::debug!("Attempting Tier 2 [320kbs-extension] for track: {}", track.id);
            match high_bitrate.resolve_stream(track).await {
                Ok(stream) => {
                    tracing::info!("Tier 2 matched! 320kbps Stream resolved.");
                    return Ok(stream);
                }
                Err(err) => {
                    tracing::warn!("Tier 2 resolution skipped: {}. Falling back to Tier 3.", err);
                }
            }
        }

        // Tier 3: YouTube Music InnerTube Opus ~160kbps (itag 251)
        tracing::debug!("Attempting Tier 3 [YouTube Music InnerTube Opus] for track: {}", track.id);
        match self.tier3_innertube.resolve_stream(track).await {
            Ok(stream) => {
                tracing::info!("Tier 3 matched! InnerTube Opus 160kbps stream active.");
                Ok(stream)
            }
            Err(err) => {
                tracing::error!("All stream tiers exhausted for track {}: {}", track.id, err);
                Err(err)
            }
        }
    }
}

#[async_trait]
impl MusicProvider for DynamicFallbackRouter {
    fn name(&self) -> &'static str {
        "EchoDynamicFallbackRouter"
    }

    fn supported_fidelity(&self) -> AudioFidelity {
        AudioFidelity::LosslessHiRes
    }

    async fn search(&self, query: &str) -> Result<Vec<Track>, ProviderError> {
        // Universal search catalog is indexed via InnerTube for exhaustive completeness
        self.tier3_innertube.search(query).await
    }

    async fn resolve_stream(&self, track: &Track) -> Result<AudioStream, ProviderError> {
        self.resolve_best_stream(track).await
    }
}
```

---

## 6. Verification, Telemetry & Quality Indicators

Echo Desktop displays real-time playback badges in the UI indicating the active fidelity tier:

| Audio Badge | Condition | Label Details |
|:---:|:---|:---|
| <span style="color:#a855f7; font-weight:bold;">HI-RES LOSSLESS</span> | `fidelity == LosslessHiRes` | `FLAC • 24-bit / 96kHz (or 192kHz)` |
| <span style="color:#3b82f6; font-weight:bold;">LOSSLESS</span> | `fidelity == LosslessCD` | `FLAC • 16-bit / 44.1kHz` |
| <span style="color:#10b981; font-weight:bold;">HIGH QUALITY</span> | `fidelity == Lossy320` | `MP3 • 320 kbps CBR` |
| <span style="color:#6b7280; font-weight:bold;">STANDARD</span> | `fidelity == LossyHigh` | `OPUS • 160 kbps (YouTube Music itag 251)` |
| <span style="color:#ef4444; font-weight:bold;">LEGACY</span> | `fidelity == LossyStandard` | `AAC • 128 kbps (YouTube Music itag 140)` |

---

## 7. Compliance, Isolation & Sandbox Guarantees

* **Storage Containment:** All provider caches, base.js AST signatures, and extension socket connections are strictly localized within `D:\Echo Music`.
* **Zero DRM Circumvention:** Standard YouTube Music audio streams are unencrypted HTTP/HTTPS DASH streams obfuscated via string manipulation (signatures) rather than DRM hardware keys (Widevine/FairPlay). Encrypted Widevine streams are ignored.
* **Safe Extension Communication:** Extensions communicate strictly via local unprivileged loopback (`127.0.0.1`) with standard JSON schemas, ensuring zero IPC attack surface.
