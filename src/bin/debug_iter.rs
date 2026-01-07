use rift::buffer::api::BufferView;
use rift::buffer::TextBuffer;

fn main() {
    let mut buffer = TextBuffer::new(1024).unwrap();
    buffer.insert_str("Hello World\nLine 2\nLine 3").unwrap();

    println!("Buffer created. Len: {}", buffer.len());

    let iter = buffer.iter_at(0);
    let collected: String = iter.map(|c| c.to_char_lossy()).collect();

    println!("Collected: {:?}", collected);

    if collected == "Hello World\nLine 2\nLine 3" {
        println!("Iteration Success!");
    } else {
        println!("Iteration Mismatch!");
    }

    // Test random access iterator
    let iter2 = buffer.iter_at(6); // "World..."
    let collected2: String = iter2.map(|c| c.to_char_lossy()).collect();
    println!("Collected from 6: {:?}", collected2);
}
