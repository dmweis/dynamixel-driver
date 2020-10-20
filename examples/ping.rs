use clap::Clap;
mod lib;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = lib::Args::parse();
    let mut driver = dynamixel_driver::DynamixelDriver::new(&args.port)?;
    for i in 0..254 {
        match driver.ping(i).await {
            Ok(()) => println!("=======> Found servo at {}", i),
            Err(_) => println!("Servo not found at {}", i),
        }
    }
    Ok(())
}
