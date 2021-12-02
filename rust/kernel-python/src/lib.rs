use kernel::eyre::Result;
use kernel_micro::{include_file, MicroKernel};

pub async fn new() -> Result<MicroKernel> {
    MicroKernel::new(
        "python",
        ("python3", "*", &["{{script}}"]),
        include_file!("python_kernel.py"),
        &[include_file!("python_codec.py")],
        "{{name}} = decode_value(\"{{json}}\")",
        "{{name}}",
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::{stencila_schema::Node, KernelTrait};
    use test_utils::{assert_json_eq, serde_json::json};

    /// Tests of basic functionality
    /// This test is replicated in all the microkernels.
    /// Other test should be written for language specific quirks and regressions.
    #[tokio::test]
    async fn basics() -> Result<()> {
        let mut kernel = new().await?;
        kernel.start().await?;

        // Assign a variable and output it
        let (outputs, messages) = kernel.exec("a = 2\na").await?;
        assert_json_eq!(messages, json!([]));
        assert_json_eq!(outputs, [2]);

        // Print the variable twice and then output it
        let (outputs, messages) = kernel.exec("print(a)\nprint(a)\na").await?;
        assert_json_eq!(messages, json!([]));
        assert_json_eq!(outputs, [2, 2, 2]);

        // Syntax error
        let (outputs, messages) = kernel.exec("bad ^ # syntax").await?;
        assert_json_eq!(messages[0].error_type, "SyntaxError");
        assert_json_eq!(messages[0].error_message, "invalid syntax (<code>, line 1)");
        assert!(messages[0].stack_trace.is_some());
        assert_json_eq!(outputs, json!([]));

        // Runtime error
        let (outputs, messages) = kernel.exec("foo").await?;
        assert_json_eq!(messages[0].error_type, "NameError");
        assert_json_eq!(messages[0].error_message, "name 'foo' is not defined");
        assert!(messages[0].stack_trace.is_some());
        assert_json_eq!(outputs, json!([]));

        // Set and get another variable
        kernel.set("b", Node::Integer(3)).await?;
        let b = kernel.get("b").await?;
        assert_json_eq!(b, 3);

        // Use both variables
        let (outputs, messages) = kernel.exec("a*b").await?;
        assert_json_eq!(messages, json!([]));
        assert_json_eq!(outputs, [6]);

        Ok(())
    }
}
