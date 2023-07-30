use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt()]
pub struct Args {
    #[structopt(about = "Serial port to use")]
    pub port: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::from_args();
    let mut driver = dynamixel_driver::DynamixelDriver::new(&args.port)?;
    for i in 0..20 {
        if let Ok(temperature) = driver.read_temperature(i).await {
            println!("servo {} has temperature of {}", i, temperature);
        }
    }
    Ok(())
}
