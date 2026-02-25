#![allow(nonstandard_style)]
use alpa::embedded_sdmmc_ram_device::{allocators};
use picoserve::routing::{post, get, delete, parse_path_segment, Router, PathRouter};
use picoserve::response::{Response, IntoResponse};
use file_manager::{init_file_manager, DummyTimesource};
use server::{CatchAll, HOME_PAGE};
use file_manager::{BlkDev, init_file_system};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    allocators::init_simulated_hardware();
    let sdcard = BlkDev::new("test_file.db").unwrap();
    init_file_manager(sdcard, DummyTimesource);

    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 8000)).await.unwrap();

    let app = std::rc::Rc::new(router());

    let config = picoserve::Config::const_default().keep_connection_alive();

    tokio::task::LocalSet::new()
        .run_until(async {
            loop {
                match init_file_system().await {
                    Ok(()) => break,
                    Err(e) => {
                        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                        println!("error: {:?}", e);
                    }
                }
            }

            loop {
                let (stream, remote_address) = listener.accept().await.unwrap();
                
                let config = config.clone();
                let app = app.clone();

                tokio::task::spawn_local(async move {
                    let mut buffer = [0u8; 2048]; 

                    match picoserve::Server::new_tokio(&app, &config, &mut buffer).serve(stream).await {
                        Ok(info) => println!("Handled {} requests from {}", info.handled_requests_count, remote_address),
                        Err(err) => println!("Error handling connection: {:?}", err),
                    }
                });
            }
        })
    .await
}

async fn home() -> impl IntoResponse {
    Response::ok(HOME_PAGE)
        .with_header("Content-Type", "text/html")
}

pub fn router() -> Router<impl PathRouter> {
    Router::new()
        .route("/", get(home))
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
}

