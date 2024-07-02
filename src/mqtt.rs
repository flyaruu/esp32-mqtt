use alloc::borrow::ToOwned;
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
use embedded_tls::{Aes128GcmSha256, Aes256GcmSha384, Certificate, NoVerify, TlsCipherSuite, TlsConfig, TlsConnection, TlsContext, TlsVerifier};
use esp_hal::rng::Rng;
use esp_wifi::wifi::{WifiDevice, WifiStaDevice};
use log::{info, warn};
use rand_core::{CryptoRng, RngCore};
use rust_mqtt::{
    client::{
        client::MqttClient,
        client_config::{ClientConfig, MqttVersion},
    },
    utils::rng_generator::CountingRng,
};
// use rust_mqtt::{
//     client::{
//         client::MqttClient,
//         client_config::{ClientConfig, MqttVersion},
//     },
//     utils::rng_generator::CountingRng,
// };

use crate::Message;

const BUFFER_SIZE: usize = 1024;

struct PseudoCrypto {
    rng: Rng
}

impl rand_core::CryptoRng for PseudoCrypto {}

impl RngCore for PseudoCrypto {
    fn next_u32(&mut self) -> u32 {
        self.rng.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.rng.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.rng.fill_bytes(dest)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.rng.try_fill_bytes(dest)
    }
}

#[task]
pub async fn send_mqtt_message(
    stack: &'static Stack<WifiDevice<'static, WifiStaDevice>>,
    receiver: Receiver<'static, NoopRawMutex, Message, 5>,
    sender: Sender<'static, NoopRawMutex, Message, 5>,
    rng: Rng
) -> ! {
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
        let state: TcpClientState<3, BUFFER_SIZE, BUFFER_SIZE> = TcpClientState::new();
        let tcp_client = TcpClient::new(stack, &state);

        let tcp_connection = tcp_client.connect(SocketAddr::new(ip, 8883)).await.unwrap();


        let mut read_record_buffer = [0; 16384];
        let mut write_record_buffer = [0; 16384];
        let cert = Certificate::X509(include_bytes!("../certificate.pem"));
        let config: TlsConfig<'_, Aes256GcmSha384> = TlsConfig::new().with_server_name(&host); //.with_cert(cert);
        // let config: TlsConfig<'_, Aes256GcmSha384> = TlsConfig::new().with_ca(cert); //.with_cert(cert);
        // TlsConfig:
        // let config: TlsConfig<'_, Aes256GcmSha384> = TlsConfig::new().with_server_name("example.com");
        let mut tls_connection: TlsConnection<_,Aes256GcmSha384> = TlsConnection::new(tcp_connection, &mut read_record_buffer, &mut write_record_buffer);
        // tls.open::<OsRng, NoVerify>(TlsContext::new(&config, &mut rng))
        // .await
        // .expect("error establishing TLS connection");
        // let verifier: TlsVerifier<Aes256GcmSha384> = TlsVerifier::new(Some(host));
        let mut crypto_rng = PseudoCrypto { rng };
        let connection_result = tls_connection.open::<PseudoCrypto,NoVerify>(TlsContext::new(
            &config,
            &mut crypto_rng,
        ))
        .await;
        if let Err(e) = connection_result {
            info!("TLS Error: {:?}",e);
        }
        // send message

        let mut send_buffer = [0_u8; BUFFER_SIZE];
        let mut receive_buffer = [0_u8; BUFFER_SIZE];
        let mut mqtt_client_config: ClientConfig<'_, 5, CountingRng> =
            ClientConfig::new(MqttVersion::MQTTv5, CountingRng(12345));
        mqtt_client_config.add_client_id("oidfsduidiodsuio");
        let mut mqtt_client = MqttClient::new(
            tls_connection,
            &mut send_buffer,
            BUFFER_SIZE,
            &mut receive_buffer,
            BUFFER_SIZE,
            mqtt_client_config,
        );
        match mqtt_client.connect_to_broker().await {
            Ok(_) => {},
            Err(err) => {
                warn!("Network error: {:?}",err);
            },
        }
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
