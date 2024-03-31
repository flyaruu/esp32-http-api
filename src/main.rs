#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(result_flattening)]

extern crate alloc;
use core::mem::MaybeUninit;
use alloc::boxed::Box;
use embassy_net::{Config, Stack, StackResources};
use embassy_time::Duration;
use esp_backtrace as _;
use esp_wifi::{EspWifiInitFor, initialize, wifi::WifiStaDevice};
use esp_hal::{clock::ClockControl, embassy::{self, executor::Executor}, peripherals::Peripherals, prelude::*, systimer::SystemTimer, timer::TimerGroup, Rng, IO};

use picoserve::{KeepAlive, ShutdownMethod, Timeouts};
use static_cell::make_static;



use esp_backtrace as _;

use crate::{net::{net_task, connection}, web::web_task};
// use esp_wifi::wifi::{WifiController, WifiDevice, WifiEvent, WifiStaDevice, WifiState};

mod net;
mod web;

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

fn init_heap() {
    const HEAP_SIZE: usize = 32 * 1024;
    static mut HEAP: MaybeUninit<[u8; HEAP_SIZE]> = MaybeUninit::uninit();
    unsafe {
        ALLOCATOR.init(HEAP.as_mut_ptr() as *mut u8, HEAP_SIZE);
    }
}


#[entry]
fn main() -> ! {
    init_heap();
    let peripherals = Peripherals::take();
    let system = peripherals.SYSTEM.split();
    let clocks = ClockControl::max(system.clock_control).freeze();
    // let mut delay = Delay::new(&clocks);

    // setup logger
    // To change the log_level change the env section in .cargo/config.toml
    // or remove it and set ESP_LOGLEVEL manually before running cargo run
    // this requires a clean rebuild because of https://github.com/rust-lang/cargo/issues/10358
    esp_println::logger::init_logger_from_env();
    log::info!("Logger is setup");

    let io = IO::new(peripherals.GPIO,peripherals.IO_MUX);

    esp_hal::interrupt::enable(esp_hal::peripherals::Interrupt::GPIO, esp_hal::interrupt::Priority::Priority1).unwrap();
    let executor = Box::leak(Box::new(Executor::new()));
    let timer_group = TimerGroup::new(peripherals.TIMG0, &clocks);    
    embassy::init(&clocks,timer_group);

    let timer = SystemTimer::new(peripherals.SYSTIMER).alarm0;
    let init = initialize(
        EspWifiInitFor::Wifi,
        timer,
        Rng::new(peripherals.RNG),
        system.radio_clock_control,
        &clocks,
    )
    .unwrap();

    let wifi = peripherals.WIFI;
    let (wifi_interface, controller) =
        esp_wifi::wifi::new_with_mode(&init, wifi, WifiStaDevice).unwrap();
    let config = Config::dhcpv4(Default::default());
    let stack_resources = Box::leak(Box::new(StackResources::<3>::new()));
    let seed = 1234; // very random, very secure seed

    let stack = Stack::new(
        wifi_interface,
        config,
        stack_resources,
        seed
    );
    let stack = Box::leak(Box::new(stack));

    let pico_config = Box::leak(Box::new(picoserve::Config {
        timeouts : Timeouts { 
            start_read_request: Some(Duration::from_secs(5)),
            read_request:Some(Duration::from_secs(1)),
            write: Some(Duration::from_secs(1)) 
        },
        connection: KeepAlive::KeepAlive,
        shutdown_method: ShutdownMethod::Shutdown,
    }));

    executor.run(|spawner| {
        spawner.spawn(connection(controller)).unwrap();
        spawner.spawn(net_task(stack)).unwrap();
        spawner.spawn(web_task(stack,pico_config)).unwrap();
    })
}
