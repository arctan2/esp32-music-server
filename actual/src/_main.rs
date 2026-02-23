#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;
mod types;

use esp_backtrace as _;
use esp_alloc as _;
use embedded_sdmmc::{SdCard, TimeSource, Timestamp};
use embedded_hal::delay::DelayNs;
use embedded_hal::spi::SpiBus;
use embedded_sdmmc::{VolumeManager};
use esp_rtos::main;
use esp_hal::system::CpuControl;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::system::Stack;
use static_cell::StaticCell;
use allocator_api2::boxed::Box;

use types::{String};
use esp_println::{println};
use embassy_executor::Spawner;
use file_manager::{ExtAlloc, init_file_system};
use esp_hal::{
    gpio::{Level, Output, OutputConfig},
    time::{Rate},
    spi::{master::{Spi, Config}, Mode},
    delay::{Delay},
};
use embedded_hal_bus::spi::ExclusiveDevice;

esp_bootloader_esp_idf::esp_app_desc!();

fn disable_sim800L_modem(gpio4: esp_hal::peripherals::GPIO4, gpio23: esp_hal::peripherals::GPIO23) {
    let mut modem_pwr = esp_hal::gpio::Output::new(
        gpio4, 
        esp_hal::gpio::Level::Low, 
        esp_hal::gpio::OutputConfig::default()
    );

    let mut modem_key = esp_hal::gpio::Output::new(
        gpio23, 
        esp_hal::gpio::Level::Low, 
        esp_hal::gpio::OutputConfig::default()
    );

    modem_pwr.set_low(); 
    modem_key.set_low();

    esp_hal::delay::Delay::new().delay_ms(500u32);
}

#[embassy_executor::task(pool_size = 1)]
async fn second_core_main() {
    server::chunks::init();

    let free_chan = server::chunks::get_free_chan().await;
    let ready_chan = server::chunks::get_ready_chan().await;

    for i in 0..server::chunks::CHAN_CAP {
        let chunk = Box::new_in(server::chunks::Chunk{ len: 0, buf: [0; server::chunks::CHUNK_SIZE] }, ExtAlloc::default());
        free_chan.send(chunk).await;
    }
    
    let mut tick = 0;
    loop {
        let fut2 = ready_chan.recv();
        let mut chunk = fut2.await;
        chunk.reset();
        free_chan.send(chunk).await;
        println!("got chunk");
        tick += 1;
    }
}

#[main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_psram(esp_hal::psram::PsramConfig::default());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);
    esp_alloc::psram_allocator!(peripherals.PSRAM, esp_hal::psram);
    static CORE1_STACK: StaticCell<Stack<{8 * 1024}>> = StaticCell::new();

    let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
    let sw = esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);

    let stack = CORE1_STACK.init(Stack::new());

    esp_rtos::start(timg0.timer0);
    esp_rtos::start_second_core(
        peripherals.CPU_CTRL,
        sw.software_interrupt0,
        sw.software_interrupt1,
        stack,
        || {
            static EXECUTOR: StaticCell<esp_rtos::embassy::Executor> = StaticCell::new();
            EXECUTOR
                .init(esp_rtos::embassy::Executor::new())
                .run(|spawner| {
                    spawner.spawn(second_core_main()).unwrap();
                });
        },
    );


    let mut tick = 0;
    
    let free_chan = server::chunks::get_free_chan().await;
    let ready_chan = server::chunks::get_ready_chan().await;

    loop{
        embassy_time::Timer::after_millis(1000).await;
        let fut2 = free_chan.recv();
        let mut chunk = fut2.await;
        ready_chan.send(chunk).await;

        println!("tick {tick}");
        embassy_time::Timer::after_millis(100).await;
        tick += 1;
    }
}

#[derive(Default)]
pub struct DummyTimesource();

impl TimeSource for DummyTimesource {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp {
            year_since_1970: 0,
            zero_indexed_month: 0,
            zero_indexed_day: 0,
            hours: 0,
            minutes: 0,
            seconds: 0,
        }
    }
}
