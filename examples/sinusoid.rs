use dynamixel_driver::DynamixelDriver;
use looprate::{ Rate, RateTimer };
use std::time::Instant;
use clap::Clap;

#[derive(Clap)]
#[clap()]
struct Args {
    #[clap(
        about = "Serial port to use"
    )]
    port: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Args = Args::parse();
    let start = Instant::now();
    let mut rate = Rate::from_frequency(200.0);
    let mut loop_rate_counter = RateTimer::new();
    let mut driver = DynamixelDriver::new(&args.port).unwrap();
    
    loop {
        rate.wait();
        driver.write_position_degrees(2, (start.elapsed().as_secs_f32()).sin() * 90.0 + 150.0)?;
        loop_rate_counter.tick();
    }
}