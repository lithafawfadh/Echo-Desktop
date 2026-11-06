use serde::Deserialize;

// --- Search Models ---

#[derive(Debug, Deserialize)]
pub struct InnerTubeSearchResponse {
    pub contents: Option<SearchContents>,
}

#[derive(Debug, Deserialize)]
pub struct SearchContents {
    #[serde(rename = "tabbedSearchResultsRenderer")]
    pub tabbed_search_results_renderer: Option<TabbedSearchResultsRenderer>,
}

#[derive(Debug, Deserialize)]
pub struct TabbedSearchResultsRenderer {
    pub tabs: Option<Vec<SearchTab>>,
}

#[derive(Debug, Deserialize)]
pub struct SearchTab {
    #[serde(rename = "tabRenderer")]
    pub tab_renderer: Option<SearchTabRenderer>,
}

#[derive(Debug, Deserialize)]
pub struct SearchTabRenderer {
    pub content: Option<SearchTabContent>,
}

#[derive(Debug, Deserialize)]
pub struct SearchTabContent {
    #[serde(rename = "sectionListRenderer")]
    pub section_list_renderer: Option<SectionListRenderer>,
}

#[derive(Debug, Deserialize)]
pub struct SectionListRenderer {
    pub contents: Option<Vec<SectionContent>>,
}

#[derive(Debug, Deserialize)]
pub struct SectionContent {
    #[serde(rename = "musicShelfRenderer")]
    pub music_shelf_renderer: Option<MusicShelfRenderer>,
}

#[derive(Debug, Deserialize)]
pub struct MusicShelfRenderer {
    pub title: Option<RunsText>,
    pub contents: Option<Vec<MusicShelfItem>>,
}

#[derive(Debug, Deserialize)]
pub struct MusicShelfItem {
    #[serde(rename = "musicResponsiveListItemRenderer")]
    pub music_responsive_list_item_renderer: Option<MusicResponsiveListItemRenderer>,
}

#[derive(Debug, Deserialize)]
pub struct MusicResponsiveListItemRenderer {
    #[serde(rename = "flexColumns")]
    pub flex_columns: Option<Vec<FlexColumn>>,
    #[serde(rename = "fixedColumns")]
    pub fixed_columns: Option<Vec<FixedColumn>>,
    pub thumbnail: Option<MusicThumbnailContainer>,
}

#[derive(Debug, Deserialize)]
pub struct FlexColumn {
    #[serde(rename = "musicResponsiveListItemFlexColumnRenderer")]
    pub music_responsive_list_item_flex_column_renderer: Option<FlexColumnRenderer>,
}

#[derive(Debug, Deserialize)]
pub struct FlexColumnRenderer {
    pub text: Option<RunsText>,
}

#[derive(Debug, Deserialize)]
pub struct FixedColumn {
    #[serde(rename = "musicResponsiveListItemFixedColumnRenderer")]
    pub music_responsive_list_item_fixed_column_renderer: Option<FixedColumnRenderer>,
}

#[derive(Debug, Deserialize)]
pub struct FixedColumnRenderer {
    pub text: Option<RunsText>,
}

#[derive(Debug, Deserialize)]
pub struct RunsText {
    pub runs: Option<Vec<RunItem>>,
}

#[derive(Debug, Deserialize)]
pub struct RunItem {
    pub text: String,
    #[serde(rename = "navigationEndpoint")]
    pub navigation_endpoint: Option<NavigationEndpoint>,
}

#[derive(Debug, Deserialize)]
pub struct NavigationEndpoint {
    #[serde(rename = "watchEndpoint")]
    pub watch_endpoint: Option<WatchEndpoint>,
    #[serde(rename = "browseEndpoint")]
    pub browse_endpoint: Option<BrowseEndpoint>,
}

#[derive(Debug, Deserialize)]
pub struct WatchEndpoint {
    #[serde(rename = "videoId")]
    pub video_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BrowseEndpoint {
    #[serde(rename = "browseId")]
    pub browse_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MusicThumbnailContainer {
    #[serde(rename = "musicThumbnailRenderer")]
    pub music_thumbnail_renderer: Option<MusicThumbnailRenderer>,
}

#[derive(Debug, Deserialize)]
pub struct MusicThumbnailRenderer {
    pub thumbnail: Option<ThumbnailSubContainer>,
}

#[derive(Debug, Deserialize)]
pub struct ThumbnailSubContainer {
    pub thumbnails: Option<Vec<ThumbnailItem>>,
}

#[derive(Debug, Deserialize)]
pub struct ThumbnailItem {
    pub url: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

// --- Player Models ---

#[derive(Debug, Deserialize)]
pub struct InnerTubePlayerResponse {
    #[serde(rename = "playabilityStatus")]
    pub playability_status: Option<PlayabilityStatus>,
    #[serde(rename = "streamingData")]
    pub streaming_data: Option<StreamingData>,
    #[serde(rename = "videoDetails")]
    pub video_details: Option<VideoDetails>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PlayabilityStatus {
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StreamingData {
    pub formats: Option<Vec<FormatItem>>,
    #[serde(rename = "adaptiveFormats")]
    pub adaptive_formats: Option<Vec<FormatItem>>,
    #[serde(rename = "expiresInSeconds")]
    pub expires_in_seconds: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FormatItem {
    pub itag: u32,
    pub url: Option<String>,
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
    pub bitrate: Option<u32>,
    #[serde(rename = "averageBitrate")]
    pub average_bitrate: Option<u32>,
    #[serde(rename = "audioSampleRate")]
    pub audio_sample_rate: Option<String>,
    #[serde(rename = "audioChannels")]
    pub audio_channels: Option<u8>,
    #[serde(rename = "contentLength")]
    pub content_length: Option<String>,
    #[serde(rename = "signatureCipher")]
    pub signature_cipher: Option<String>,
    pub cipher: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct VideoDetails {
    #[serde(rename = "videoId")]
    pub video_id: String,
    pub title: String,
    pub author: String,
    #[serde(rename = "lengthSeconds")]
    pub length_seconds: Option<String>,
}
