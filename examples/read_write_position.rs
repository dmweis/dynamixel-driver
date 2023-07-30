use dynamixel_driver::DynamixelDriver;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt()]
pub struct Args {
    #[structopt(about = "Serial port to use")]
    pub port: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::from_args();
    let mut driver = dynamixel_driver::DynamixelDriver::new(&args.port)?;
    loop {
        if let Err(error) = do_loop(&mut driver).await {
            println!("Failed loop with {}", error);
        }
    }
}

async fn do_loop(driver: &mut DynamixelDriver) -> anyhow::Result<()> {
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
