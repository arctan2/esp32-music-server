use picoserve::routing::{post, get, delete, parse_path_segment, PathRouter, Router};
use picoserve::response::{IntoResponse, Response};
use picoserve::extract::{Json};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use crate::event_handler::{Event, EVENT_CHAN};
use crate::types::{WifiSsidPwd, WifiStatus};
use server::{CatchAll, HOME_PAGE};
use crate::types::String;

static CONFIG_PAGE: &str = include_str!("./html/config.html");
static STATUS_SIGNAL: Signal<CriticalSectionRawMutex, WifiStatus> = Signal::new();
static FLASH_DATA_SIGNAL: Signal<CriticalSectionRawMutex, WifiSsidPwd> = Signal::new();

async fn set_config(Json(data): Json<WifiSsidPwd>) -> impl IntoResponse {
    EVENT_CHAN.send(Event::SetConfig(data)).await;
    EVENT_CHAN.send(Event::Connect).await;
    "set_config: config set"
}

async fn write_to_flash(Json(data): Json<WifiSsidPwd>) -> impl IntoResponse {
    EVENT_CHAN.send(Event::WriteConfigToFlash(data)).await;
    "write_to_flash: wrote"
}

async fn connect() -> impl IntoResponse {
    EVENT_CHAN.send(Event::Connect).await;
    "connect: connected"
}

async fn disconnect() -> impl IntoResponse {
    EVENT_CHAN.send(Event::Disconnect).await;
    "disconnect: disconnected"
}

async fn software_reset() -> impl IntoResponse {
    EVENT_CHAN.send(Event::SoftwareReset).await;
    "software_reset: reset done"
}

async fn status() -> impl IntoResponse {
    STATUS_SIGNAL.reset();
    EVENT_CHAN.send(Event::GetStatus(&STATUS_SIGNAL)).await;
    let status = STATUS_SIGNAL.wait().await;
    picoserve::response::json::Json(status)
}

async fn get_flash_data() -> impl IntoResponse {
    FLASH_DATA_SIGNAL.reset();
    EVENT_CHAN.send(Event::GetFlashData(&FLASH_DATA_SIGNAL)).await;
    let data = FLASH_DATA_SIGNAL.wait().await;
    picoserve::response::json::Json(data)
}

async fn print_alloc() -> impl IntoResponse {
    STATUS_SIGNAL.reset();
    EVENT_CHAN.send(Event::GetStatus(&STATUS_SIGNAL)).await;
    let status = STATUS_SIGNAL.wait().await;
    let stats: esp_alloc::HeapStats = esp_alloc::HEAP.stats();
    esp_println::println!("{}", stats);
    picoserve::response::json::Json(status)
}

async fn home() -> impl IntoResponse {
    Response::ok(HOME_PAGE)
        .with_header("Content-Type", "text/html")
}

async fn config() -> impl IntoResponse {
    Response::ok(CONFIG_PAGE).with_header("Content-Type", "text/html")
}

pub fn router() -> Router<impl PathRouter> {
    Router::new()
        .route("/", get(home))
        .route("/config", get(config))
        .route("/set-config", post(set_config))
        .route("/write-to-flash", post(write_to_flash))
        // .route("/connect", get(connect))
        // .route("/disconnect", get(disconnect))
        .route("/status", get(status))
        // .route("/software-reset", get(software_reset))
        .route("/get-flash-data", get(get_flash_data))
        .route("/print-alloc", get(print_alloc))

        .nest("/music", Router::new()
            .route("/list", get(server::handle_music_list))
            .route(("/delete", parse_path_segment::<String>()), delete(server::delete::handle_delete_music))
            // .route(("/info", parse_path_segment::<String>()), get(server::handle_music_info))
            // .route(("/data", parse_path_segment::<String>()), get(server::handle_music_data))
            .route("/upload-new", post(server::upload::new))
            .route("/upload-chunk", post(server::upload::chunk))
            .route("/upload-end", post(server::upload::end))
        )
        .route("/db", delete(server::delete::handle_delete_db))
        .route(("/fs", CatchAll), get(server::handle_fs))
        .route(("/fs-music-delete", parse_path_segment::<String>()), delete(server::delete::handle_fs_music_delete))
}

