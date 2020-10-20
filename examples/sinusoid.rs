use clap::Clap;
use dynamixel_driver::DynamixelDriver;
use std::time::Instant;
mod lib;
use tokio::time::{delay_for, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = lib::Args::parse();

    let start = Instant::now();
    let mut driver = dynamixel_driver::DynamixelDriver::new(&args.port)?;

    loop {
        delay_for(Duration::from_millis(100)).await;
        driver
            .write_position_degrees(1, (start.elapsed().as_secs_f32()).sin() * 90.0 + 150.0)
            .await?;
    }
}
