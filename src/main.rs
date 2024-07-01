#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

extern crate alloc;
use alloc::{borrow::ToOwned, boxed::Box, string::String, vec::Vec};
use esp_hal_embassy::Executor;
use core::{mem::MaybeUninit, str::from_utf8};
use embassy_executor::task;
use embassy_net::{Config, Stack, StackResources};
use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    channel::{Channel, Receiver, Sender},
};
use embassy_time::Timer;
use esp_backtrace as _;
use esp_hal::{clock::ClockControl, peripherals::Peripherals, rng::Rng, system::SystemControl, timer::{systimer::SystemTimer, timg::TimerGroup}};
use esp_println::println;

use esp_wifi::{
    initialize,
    wifi::{new_with_mode, WifiStaDevice},
    EspWifiInitFor,
};

use log::info;
use net::run_network;

use crate::{mqtt::send_mqtt_message, net::connect};

mod mqtt;
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

type Message = (String, Vec<u8>);

fn main() -> ! {
    init_heap();
    let peripherals = Peripherals::take();
    let system = SystemControl::new(peripherals.SYSTEM);
    let clocks = ClockControl::max(system.clock_control).freeze();

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
        peripherals.RADIO_CLK,
        &clocks,
    )
    .unwrap();
    let wifi = peripherals.WIFI;
    let (device, controller) = new_with_mode(&init, wifi, WifiStaDevice).unwrap();
    let dhcpconfig = Config::dhcpv4(Default::default());
    let stack_resource = Box::leak(Box::new(StackResources::<5>::new()));
    let stack = Stack::new(device, dhcpconfig, stack_resource, 3845834);

    let stack = Box::leak(Box::new(stack));

    let executor = Box::leak(Box::new(Executor::new()));
    let timer_group = TimerGroup::new_async(peripherals.TIMG0, &clocks);

    let outbox_channel: Channel<NoopRawMutex, Message, 5> = Channel::new();
    let outbox_channel = Box::leak(Box::new(outbox_channel));
    let inbox_channel: Channel<NoopRawMutex, Message, 5> = Channel::new();
    let inbox_channel = Box::leak(Box::new(inbox_channel));
    esp_hal_embassy::init(&clocks, timer_group);
    executor.run(|spawner| {
        spawner.spawn(connect(controller)).unwrap();
        spawner.spawn(run_network(stack)).unwrap();
        spawner
            .spawn(send_mqtt_message(
                stack,
                outbox_channel.receiver(),
                inbox_channel.sender(),
            ))
            .unwrap();
        spawner
            .spawn(read_sensor_data(
                outbox_channel.sender(),
                inbox_channel.receiver(),
            ))
            .unwrap();
    })
}

#[task]
async fn read_sensor_data(
    sender: Sender<'static, NoopRawMutex, Message, 5>,
    config_receiver: Receiver<'static, NoopRawMutex, Message, 5>,
) -> ! {
    let mut delay = 5_u64;
    loop {
        if let Ok((_, payload)) = config_receiver.try_receive() {
            delay = from_utf8(&payload).unwrap().parse::<u64>().unwrap_or(5);
        }
        sender
            .send(("esp32_test_topic".to_owned(), "abc".as_bytes().to_vec()))
            .await;
        info!("Sending from sensor");
        Timer::after_secs(delay).await;
    }
}
