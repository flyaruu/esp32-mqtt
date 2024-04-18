use alloc::format;
use embassy_executor::task;
use embassy_net::{dns::DnsSocket, tcp::client::{TcpClient, TcpClientState}, Stack};
use embassy_time::Timer;
use embedded_nal_async::{AddrType, Dns, SocketAddr, TcpConnect};
use esp_wifi::wifi::{WifiDevice, WifiStaDevice};
use log::info;
use rust_mqtt::{client::{client::MqttClient, client_config::{ClientConfig, MqttVersion}}, utils::rng_generator::CountingRng};

const BUFFER_SIZE: usize = 1024;

#[task]
pub async fn send_mqtt_message(stack: &'static Stack<WifiDevice<'static,WifiStaDevice>> )->! {
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
        let mqtt_client_config: ClientConfig<'_, 5, CountingRng> = ClientConfig::new(MqttVersion::MQTTv5, CountingRng(12345));
        let mut mqtt_client = MqttClient::new(tcp_connection, &mut send_buffer, BUFFER_SIZE, &mut receive_buffer, BUFFER_SIZE, mqtt_client_config);
        mqtt_client.connect_to_broker().await.unwrap();
        mqtt_client.send_message("esp32_test_topic", format!("Hello there: {}",counter).as_bytes(), rust_mqtt::packet::v5::publish_packet::QualityOfService::QoS1, false).await.unwrap();
        Timer::after_secs(5).await;
        info!("Iteration completed");
        counter+=1;
    }
}