use alloc::{borrow::ToOwned, format};
use embassy_executor::task;
use embassy_futures::select::{select, Either};
use embassy_net::{
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
    Stack,
};
use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    channel::{Receiver, Sender},
};
use embassy_time::Timer;
use embedded_nal_async::{AddrType, Dns, SocketAddr, TcpConnect};
use esp_wifi::wifi::{WifiDevice, WifiStaDevice};
use log::info;
use rust_mqtt::{
    client::{
        client::MqttClient,
        client_config::{ClientConfig, MqttVersion},
    },
    utils::rng_generator::CountingRng,
};

use crate::Message;

const BUFFER_SIZE: usize = 1024;

#[task]
pub async fn send_mqtt_message(
    stack: &'static Stack<WifiDevice<'static, WifiStaDevice>>,
    receiver: Receiver<'static, NoopRawMutex, Message, 5>,
    sender: Sender<'static, NoopRawMutex, Message, 5>,
) -> ! {
    let mut counter = 0;
    loop {
        if !stack.is_link_up() {
            Timer::after_millis(500).await;
            continue;
        }
        // resolve host
        let host = "mqtt.eclipseprojects.io";
        let dns_socket = DnsSocket::new(stack);
        let ip = loop {
            if let Ok(ip) = dns_socket.get_host_by_name(host, AddrType::Either).await {
                break ip;
            }
            Timer::after_millis(500).await;
        };
        let mut state: TcpClientState<3, BUFFER_SIZE, BUFFER_SIZE> = TcpClientState::new();
        let tcp_client = TcpClient::new(stack, &mut state);

        let tcp_connection = tcp_client.connect(SocketAddr::new(ip, 1883)).await.unwrap();
        // send message

        let mut send_buffer = [0_u8; BUFFER_SIZE];
        let mut receive_buffer = [0_u8; BUFFER_SIZE];
        let mut mqtt_client_config: ClientConfig<'_, 5, CountingRng> =
            ClientConfig::new(MqttVersion::MQTTv5, CountingRng(12345));
        mqtt_client_config.add_client_id("oidfsduidiodsuio");
        let mut mqtt_client = MqttClient::new(
            tcp_connection,
            &mut send_buffer,
            BUFFER_SIZE,
            &mut receive_buffer,
            BUFFER_SIZE,
            mqtt_client_config,
        );
        mqtt_client.connect_to_broker().await.unwrap();
        mqtt_client
            .subscribe_to_topic("esp32_test_configuration")
            .await
            .unwrap();
        loop {
            info!("Loop!");
            match select(receiver.receive(), mqtt_client.receive_message()).await {
                Either::First((topic, payload)) => mqtt_client
                    .send_message(
                        &topic,
                        &payload,
                        rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS1,
                        false,
                    )
                    .await
                    .unwrap(),
                Either::Second(result) => match result {
                    Ok((topic, payload)) => {
                        info!("Configuration found: {}", topic);
                        sender.send((topic.to_owned(), payload.to_vec())).await;
                    }
                    Err(_) => break,
                },
            }
        }
    }
}
