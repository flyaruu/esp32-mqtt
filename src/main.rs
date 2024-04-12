#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

extern crate alloc;
use core::mem::MaybeUninit;
use alloc::boxed::Box;
use embassy_executor::task;
use embassy_net::{Config, Stack, StackResources};
use embassy_time::Timer;
use esp_backtrace as _;
use esp_hal::{clock::ClockControl, embassy::{self, executor::Executor}, peripherals::Peripherals, prelude::*, timer::TimerGroup, Delay};
use esp_println::println;

use esp_wifi::{initialize, wifi::{new_with_mode, WifiStaDevice}, EspWifiInitFor};

use esp_hal::{systimer::SystemTimer, Rng};
use net::run_network;

use crate::net::connect;

mod net;


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
    let mut delay = Delay::new(&clocks);

    // setup logger
    // To change the log_level change the env section in .cargo/config.toml
    // or remove it and set ESP_LOGLEVEL manually before running cargo run
    // this requires a clean rebuild because of https://github.com/rust-lang/cargo/issues/10358
    esp_println::logger::init_logger_from_env();
    log::info!("Logger is setup");
    println!("Hello world!");
    let timer = SystemTimer::new(peripherals.SYSTIMER).alarm0;
    let init = initialize(
        EspWifiInitFor::Wifi,
        timer,
        Rng::new(peripherals.RNG),
        system.radio_clock_control,
        &clocks,
    )
    .unwrap();
    let wifi =  peripherals.WIFI;
    let (device,controller) = new_with_mode(&init, wifi, WifiStaDevice).unwrap();
    let dhcpconfig = Config::dhcpv4(Default::default());
    let stack_resource = Box::leak(Box::new(StackResources::<5>::new()));
    let stack = Stack::new(device,dhcpconfig,stack_resource,3845834);

    let stack = Box::leak(Box::new(stack));
    
    let executor = Box::leak(Box::new(Executor::new()));
    let timer_group = TimerGroup::new(peripherals.TIMG0, &clocks);
    embassy::init(&clocks,timer_group);
    executor.run(|spawner| {
        spawner.spawn(print_stuff()).unwrap();
        spawner.spawn(connect(controller)).unwrap();
        spawner.spawn(run_network(stack)).unwrap();
    })
}

#[task]
async fn print_stuff() {
    loop {
        println!("Hello!");
        Timer::after_secs(1).await;        
    }
}
