fn main() {
    println!("This program will organize flows. It will recieve serialized flows.");
    println!("After typchecking, it will:");
    println!("- create ephemeral channels connecting nodes in the flow");
    println!("- push the the job queue for each node in the flow");
    println!("- propagate flow panics");
    println!("- cleanup after finished or panicked flows");
    println!("- somehow report status and results, possibly stream values back to callers");
}
