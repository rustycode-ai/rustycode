// Basic usage example for rustycode-id
#![allow(clippy::expect_used)]

use rustycode_id::{EventId, MemoryId, SessionId};

fn main() {
    println!("=== RustyCode Sortable ID System ===\n");

    // Creating different ID types
    println!("1. Creating IDs:");
    let session_id = SessionId::new();
    let event_id = EventId::new();
    let memory_id = MemoryId::new();

    println!("   Session ID: {session_id}");
    println!("   Event ID:   {event_id}");
    println!("   Memory ID:  {memory_id}");

    // ID properties
    println!("\n2. ID Properties:");
    println!(
        "   Length: {} chars (UUID: 36 chars)",
        session_id.to_string().len()
    );
    println!("   Prefix: {}", session_id.inner().prefix());
    println!("   Timestamp: {}", session_id.timestamp());

    // Time-sortable demonstration
    println!("\n3. Time-Sortable IDs:");
    std::thread::sleep(std::time::Duration::from_millis(10));
    let id1 = SessionId::new();
    std::thread::sleep(std::time::Duration::from_millis(10));
    let id2 = SessionId::new();
    std::thread::sleep(std::time::Duration::from_millis(10));
    let id3 = SessionId::new();

    println!("   ID 1: {id1} (earliest)");
    println!("   ID 2: {id2}");
    println!("   ID 3: {id3} (latest)");
    println!(
        "   Sorted: id1 < id2 < id3: {}",
        id1.to_string() < id2.to_string() && id2.to_string() < id3.to_string()
    );

    // Parsing IDs
    println!("\n4. Parsing IDs:");
    let id_str = session_id.to_string();
    let parsed = SessionId::parse(&id_str).expect("valid session ID should parse");
    println!("   Original: {session_id}");
    println!("   Parsed:   {parsed}");
    println!("   Match: {}", session_id.to_string() == parsed.to_string());

    // Serde support
    println!("\n5. Serde Serialization:");
    let json = serde_json::to_string_pretty(&session_id).expect("serialization should succeed");
    println!("   JSON: {json}");

    let deserialized: SessionId =
        serde_json::from_str(&json).expect("deserialization should succeed");
    println!("   Deserialized: {deserialized}");
    println!(
        "   Match: {}",
        session_id.to_string() == deserialized.to_string()
    );

    // Type safety
    println!("\n6. Type Safety:");
    println!("   SessionId prevents parsing EventId:");
    let event_str = event_id.to_string();
    let wrong_parse = SessionId::parse(&event_str);
    println!("   Parsing event as session: {wrong_parse:?}");

    // Bulk ID generation
    println!("\n7. Bulk Generation (1000 IDs):");
    let start = std::time::Instant::now();
    let mut ids = std::collections::HashSet::new();
    for _ in 0..1000 {
        let id = SessionId::new();
        ids.insert(id.to_string());
    }
    let elapsed = start.elapsed();
    println!("   Generated {} unique IDs in {elapsed:?}", ids.len());
    println!("   All unique: {}", ids.len() == 1000);

    println!("\n=== Demo Complete ===");
}
