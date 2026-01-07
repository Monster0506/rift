use rift::buffer::TextBuffer;
use rift::search::find_all;
use std::time::Instant;

fn main() {
    println!("Preparing benchmark buffer...");
    // Create a 10MB buffer roughly
    let mut buffer = TextBuffer::new(1024).expect("Failed to create buffer");

    let line = "fn sample_function(x: i32, y: i32) -> i32 { println!(\"debug: {}\", x + y); return x * y; }\n";
    // 90 chars per line. 100,000 lines => 9MB.
    let count = 1_000;

    let mut content = String::with_capacity(count * line.len());
    for _ in 0..count {
        content.push_str(line);
    }

    println!("Inserting into buffer...");
    let start_insert = Instant::now();
    buffer
        .insert_str(&content)
        .expect("Failed to insert string");
    println!("Insertion took: {:?}", start_insert.elapsed());
    println!("Buffer size: {} chars", buffer.len());

    println!("\n--- Benchmarks ---\n");

    // 1. Simple literal (common)
    run_bench(&buffer, "println");

    // 2. Simple literal (rare)
    run_bench(&buffer, "nonexistentstring");

    // 3. Regex simple
    run_bench(&buffer, "fn\\s+sample");

    // 4. Regex wildcard
    run_bench(&buffer, "return.*y");

    // 5. Anchor
    run_bench(&buffer, "^fn");
}

fn run_bench(buffer: &TextBuffer, pattern: &str) {
    println!("Running search for pattern: '{}'", pattern);
    let start = Instant::now();
    let res = find_all(buffer, pattern);
    let total_duration = start.elapsed();

    match res {
        Ok((matches, stats)) => {
            println!("  Total Wall Time: {:?}", total_duration);
            println!(
                "  Internal Stats: Compile: {:?}, Index: {:?}, Search: {:?}",
                stats.compilation_time, stats.index_time, stats.search_time
            );
            println!("  Match Count: {}", matches.len());
        }
        Err(e) => println!("  Error: {:?}", e),
    }
    println!("------------------------------------------------");
}
