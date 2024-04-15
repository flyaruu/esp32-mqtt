#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

extern crate alloc;
use core::mem::MaybeUninit;
use alloc::{borrow::ToOwned, boxed::Box, format, string::String};
use embassy_executor::task;
use embassy_net::{Config, Stack, StackResources};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::{Channel, Receiver, Sender}};
use embassy_time::Timer;
use esp_backtrace as _;
use esp_hal::{clock::ClockControl, embassy::{self, executor::Executor}, peripherals::Peripherals, prelude::*, timer::TimerGroup};
use esp_println::println;

use esp_wifi::{initialize, wifi::{new_with_mode, WifiStaDevice}, EspWifiInitFor};

use esp_hal::{systimer::SystemTimer, Rng};
use net::run_network;

use crate::{mqtt::send_mqtt, net::connect};

mod net;
mod mqtt;


#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

fn init_heap() {
    const HEAP_SIZE: usize = 32 * 1024;
    static mut HEAP: MaybeUninit<[u8; HEAP_SIZE]> = MaybeUninit::uninit();

    unsafe {
        ALLOCATOR.init(HEAP.as_mut_ptr() as *mut u8, HEAP_SIZE);
    }
}
type Message = (String,String);
type MessageChannel = Channel<NoopRawMutex, Message, 10>;
type MessageSender = Sender<'static, NoopRawMutex, Message, 10>;
type MessageReceiver = Receiver<'static, NoopRawMutex, Message, 10>;

#[entry]
fn main() -> ! {
    init_heap();
    let peripherals = Peripherals::take();
    let system = peripherals.SYSTEM.split();

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

    let message_channel = Box::leak(Box::new(MessageChannel::new()));

    
    executor.run(|spawner| {
        spawner.spawn(connect(controller)).unwrap();
        spawner.spawn(run_network(stack)).unwrap();
        spawner.spawn(send_mqtt(stack,"mqtt.eclipseprojects.io", message_channel.receiver())).unwrap();
        spawner.spawn(message_producer(message_channel.sender())).unwrap()
    })
}

#[task]
async fn message_producer(sender: MessageSender) {
    let mut i = 0;
    loop {
        sender.send(("floodplain".to_owned(),format!("Message #: {}",i))).await;
        i+=1;
        Timer::after_secs(5).await;
    }
}