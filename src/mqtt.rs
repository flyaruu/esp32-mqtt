use alloc::format;
use embassy_executor::task;
use embassy_net::{
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
    Stack,
};
use embassy_time::Timer;
use embedded_nal_async::{AddrType, Dns, SocketAddr, TcpConnect};
use esp_wifi::wifi::{WifiDevice, WifiStaDevice};
use log::{info, warn};
use rust_mqtt::{
    client::{
        client::MqttClient,
        client_config::{ClientConfig, MqttVersion},
    },
    packet::v5::publish_packet::QualityOfService,
    utils::rng_generator::CountingRng,
};

use crate::MessageReceiver;

#[task]
pub async fn send_mqtt(
    stack: &'static Stack<WifiDevice<'static, WifiStaDevice>>,
    host: &'static str,
    message_receiver: MessageReceiver,
) -> ! {
    const BUFFER_SIZE: usize = 128;
    let mut counter = 0;
    loop {
        info!("Starting MQTT task");
        if !stack.is_link_up() {
            Timer::after_secs(2).await;
            continue;
        }
        info!("link up!");

        let dns_socket = DnsSocket::new(stack);
        let ip = loop {
            match dns_socket.get_host_by_name(host, AddrType::Either).await {
                Ok(ip) => break ip,
                Err(_) => {}
            }
            info!("failed dns, retrying");
            Timer::after_secs(1).await;
        };
        let socket_address = SocketAddr::new(ip, 1883);

        info!("Resolved to address: {:?}", socket_address);
        let mut tcp_state: TcpClientState<5, 1024, 1024> = TcpClientState::new();
        let tcp_client = TcpClient::new(stack, &mut tcp_state);

        let tcp_connection = tcp_client.connect(socket_address).await.unwrap();

        let config: ClientConfig<'_, 5, _> =
            ClientConfig::new(MqttVersion::MQTTv5, CountingRng(30000));
        // config.add_client_id("floodplain");
        // config.max_packet_size = 100;
        let mut receive_buffer = [0; BUFFER_SIZE];
        let mut send_buffer = [0; BUFFER_SIZE];
        let mut mqtt_client = MqttClient::new(
            tcp_connection,
            &mut send_buffer,
            BUFFER_SIZE,
            &mut receive_buffer,
            BUFFER_SIZE,
            config,
        );
        mqtt_client.connect_to_broker().await.unwrap();

        loop {
            let (topic, body) = message_receiver.receive().await;
            match mqtt_client
                .send_message(&topic, body.as_bytes(), QualityOfService::QoS1, false)
                .await
            {
                Ok(()) => {}
                Err(_) => {
                    warn!("Send failed");
                    break;
                }
            }
        }
        info!("Connected!");
        for _ in 0..10 {
            mqtt_client
                .send_message(
                    "floodplain",
                    format!("some_content: {}", counter).as_bytes(),
                    QualityOfService::QoS1,
                    false,
                )
                .await
                .unwrap();
            counter += 1;
            info!("Message sent!");
            Timer::after_secs(5).await;
        }
    }
}
