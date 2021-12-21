use std::{thread::sleep, time::Duration};
mod lib;
use structopt::StructOpt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = lib::Args::from_args();
    let mut driver = dynamixel_driver::DynamixelDriver::new(&args.port)?;
    let commands = vec![(1, 1023), (2, 1023)];
    driver.sync_write_position(commands).await?;
    sleep(Duration::from_secs(2));
    let commands = vec![(1, 0), (2, 0)];
    driver.sync_write_position(commands).await?;
    Ok(())
}
