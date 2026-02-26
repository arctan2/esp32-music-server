use esp_println::println;
use static_cell::StaticCell;
use embassy_net::Stack;
use embassy_time::Duration;
use embassy_net::tcp::TcpSocket;
use crate::router;
use allocator_api2::boxed::Box;

#[embassy_executor::task(pool_size = 2)]
pub async fn http_server_task(
    stack: Stack<'static>,
    static_resources: &'static StaticCell<([u8; 2048], [u8; 2048], [u8; 2048])>
) {
    let (rx_buf, tx_buf, http_buf) = static_resources.init(([0; 2048], [0; 2048], [0; 2048]));
    let app = router::router();
    let config = picoserve::Config::new(picoserve::Timeouts {
        start_read_request: Some(Duration::from_secs(2)),
        persistent_start_read_request: Some(Duration::from_secs(5)),
        read_request: Some(Duration::from_secs(10)),
        write: Some(Duration::from_secs(5)),
    }).keep_connection_alive();

    println!("HTTP Server listening on port 80...");

    loop {
        let mut socket = TcpSocket::new(stack, rx_buf, tx_buf);
        socket.set_timeout(Some(Duration::from_secs(5)));

        if let Ok(_) = embassy_time::with_timeout(Duration::from_secs(20), socket.accept(80)).await {
            let _ = embassy_time::with_timeout(
                Duration::from_secs(10), 
                picoserve::Server::new(&app, &config, http_buf).serve(socket)
            ).await;
        } else {
            // important to free the stuff of wifi. Else the wifi gets clogged up. It's fixing the
            // wifi disappearing problem
            socket.abort();
        }
    }
}

