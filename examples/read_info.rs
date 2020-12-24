mod lib;
use clap::Clap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = lib::Args::parse();
    let mut driver = dynamixel_driver::DynamixelDriver::new(&args.port)?;
    for i in 0..20 {
        if driver.ping(i).await.is_ok() {
            println!("Servo id: {}", i);
            if let Ok(temperature) = driver.read_temperature(i).await {
                println!("   temperature of {}", temperature);
            }
            if let Ok(position) = driver.read_position_degrees(i).await {
                println!("   position degrees of {}", position);
            }
        }
    }
    Ok(())
}
