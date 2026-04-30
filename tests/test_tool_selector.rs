use rustycode_tools::ToolProfile;

fn main() {
    let test = "Show me the main function";
    let result = ToolProfile::from_prompt(test);
    println!("Test: '{}'", test);
    println!("Result: {:?}", result);
    println!("Expected: {:?}", ToolProfile::Explore);
}
