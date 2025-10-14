use std::net::UdpSocket;
use std::thread::sleep;
use std::time::Duration;

fn main() -> std::io::Result<()> {
    // Create UDP socket
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_nonblocking(true)?;
    
    let qspy_addr = "127.0.0.1:7701";
    
    println!("Sending test packets to QSpy at {}...", qspy_addr);
    
    for i in 0..10 {
        // Build test packet: seq_num + record_type + simple data
        let packet = vec![i as u8, 0x01, 0xAA, 0xBB, 0xCC, 0xDD];
        
        match socket.send_to(&packet, qspy_addr) {
            Ok(n) => println!("Sent {} bytes in packet {}", n, i),
            Err(e) => eprintln!("Failed to send packet {}: {}", i, e),
        }
        
        sleep(Duration::from_millis(100));
    }
    
    println!("Done sending test packets");
    Ok(())
}
