use dynamixel_driver;

fn main() {
    let mut driver = dynamixel_driver::DynamixelDriver::new("COM11").unwrap();
    for i in 0..20 {
        if let Ok(temperature) = driver.read_temperature(i) {
            println!("servo {} has temperature of {}", i, temperature);
        }
    }
}