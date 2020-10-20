mod lib;
use clap::Clap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = lib::Args::parse();
    let mut driver = dynamixel_driver::DynamixelDriver::new(&args.port)?;
    loop {
        driver.write_position_degrees(1, 100.0).await?;
        loop {
            let pos = driver.read_position_degrees(1).await?;
            if pos < 101.0 {
                break;
            }
        }
        driver.write_position_degrees(1, 200.0).await?;
        loop {
            let pos = driver.read_position_degrees(1).await?;
            if pos > 199.0 {
                break;
            }
        }
    }
}
