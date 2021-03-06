use clap::Clap;
use std::time::Instant;
mod lib;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = lib::Args::parse();

    let start = Instant::now();
    let mut driver = dynamixel_driver::DynamixelDriver::new(&args.port)?;

    loop {
        sleep(Duration::from_millis(10)).await;
        driver
            .write_position_degrees(1, (start.elapsed().as_secs_f32()).sin() * 90.0 + 150.0)
            .await?;
    }
}
