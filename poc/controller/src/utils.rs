#[macro_export]
macro_rules! execution_time {
    ($code:block) => {{
        let start = std::time::Instant::now();
        let res = $code;
        let execution_time = start.elapsed();
        (res, execution_time)
    }};
}
