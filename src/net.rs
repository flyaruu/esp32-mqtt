use embassy_executor::task;
use embassy_net::Stack;
use embassy_time::Timer;
use esp_wifi::wifi::{get_wifi_state, ClientConfiguration, Configuration, WifiController, WifiDevice, WifiStaDevice, WifiState};
use log::info;

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASSWORD");

#[task]
pub async fn connect(mut controller: WifiController<'static>) {
    loop {
        let state = get_wifi_state();
        info!("Current state: {:?}",state);
        match state {
            esp_wifi::wifi::WifiState::StaConnected=>{
                info!("connected");
                controller.wait_for_event(esp_wifi::wifi::WifiEvent::StaDisconnected).await;
                Timer::after_secs(5).await;
            },
            esp_wifi::wifi::WifiState::StaStarted => info!("Started"),
            esp_wifi::wifi::WifiState::StaDisconnected => info!("Disconnected"),
            esp_wifi::wifi::WifiState::StaStopped =>info!("Stopped"),
            _ => {}
        }
        let state = get_wifi_state();
        info!("Current state: {:?}",state);
        match state  {
            WifiState::Invalid | WifiState::StaStopped=> {
                let client_config = Configuration::Client(ClientConfiguration {
                    ssid : SSID.try_into().unwrap(),
                    password : PASSWORD.try_into().unwrap(),
                    ..Default::default()
                });
                controller.set_configuration(&client_config).unwrap();
                info!("About to start");
                controller.start().await.unwrap();
                info!("Started");
            },
            _ => {}
        }

        let state = get_wifi_state();
        info!("Now: {:?}",state);

        match controller.connect().await {
            Ok(_) => info!("Wifi up"),
            Err(e) => {
                info!("wifi down: {:?}",e);
                Timer::after_millis(500).await;
            },
        }
   }
}

#[task]
pub async fn run_network(stack: &'static Stack<WifiDevice<'static,WifiStaDevice>> ) {
    stack.run().await;
}