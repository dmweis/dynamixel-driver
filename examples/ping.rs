use dynamixel_driver;

fn main() {
    let mut driver = dynamixel_driver::DynamixelDriver::new("COM11").unwrap();
    for i in 0..20 {
        if driver.ping(i).is_ok() {
            println!("Found servo at {}", i);
        } else {
            println!("Not found as {}", i);
        }
    }
    // driver.ping(9).unwrap();
}