mod lib;
use structopt::StructOpt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = lib::Args::from_args();
    let mut driver = dynamixel_driver::DynamixelDriver::new(&args.port)?;
    driver.write_id(2, 1).await.unwrap();
    Ok(())
}
