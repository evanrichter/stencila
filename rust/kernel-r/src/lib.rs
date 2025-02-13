use std::process::Command;

use kernel_micro::{include_file, MicroKernel};

/// A microkernel for R
pub fn new() -> MicroKernel {
    MicroKernel::new(
        "r-micro",
        &["r"],
        true,
        cfg!(not(target_os = "windows")),
        is_forkable(),
        ("Rscript", "*"),
        &["{{script}}"],
        include_file!("r-kernel.r"),
        &[include_file!("r-codec.r")],
        "{{name}} <- decode_value(r\"({{json}})\")",
        "cat(encode_value({{name}}, unbox = TRUE))",
    )
}

/// Check to see if the installation of R allows for forking of microkernel
fn is_forkable() -> bool {
    if cfg!(target_os = "windows") {
        false
    } else {
        // Attempt to run R and see if Cairo package is installed.
        // If this fails, or if it is not, returns false
        Command::new("R")
            .args([
                "--slave",
                "-e",
                "quit(status=!nzchar(system.file(package='Cairo')))",
            ])
            .spawn()
            .and_then(|mut child| child.wait())
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use kernel::{
        common::{
            eyre::{bail, Result},
            once_cell::sync::Lazy,
            tokio::{self, sync::Mutex},
        },
        stencila_schema::{Array, Node, Number, Object, Primitive},
        KernelTrait, TaskResult,
    };
    use test_utils::{assert_json_eq, assert_json_is};

    use super::*;

    // Make sure that only one test runs at any one time
    // This is to avoid `install.packages` being run in parallel.
    // Previous the `serial_test` crate was used for this but did not
    // seem to provide necessary guarantee.
    static QUEUE: Lazy<Mutex<()>> = Lazy::new(Mutex::default);

    async fn skip_or_kernel() -> Result<MicroKernel> {
        let mut kernel = new();
        if !kernel.is_available().await {
            eprintln!("R not available on this machine");
            bail!("Skipping")
        } else {
            kernel.start_here().await?;
        }

        Ok(kernel)
    }

    // Run these tests serially to avoid parallel installation of dependencies
    // which may interfere with each other.

    /// Tests of basic functionality
    /// This test is replicated in all the microkernels.
    /// Other test should be written for language specific quirks and regressions.
    #[tokio::test]
    async fn basics() -> Result<()> {
        let _guard = QUEUE.lock().await;

        let mut kernel = match skip_or_kernel().await {
            Ok(kernel) => kernel,
            Err(..) => return Ok(()),
        };

        // The execution context should start off empty
        let (outputs, messages) = kernel.exec("ls()", None).await?;
        assert_json_is!(messages, []);
        assert_json_is!(outputs, [[]]);

        // Assign a variable and output it
        let (outputs, messages) = kernel.exec("a <- 2\na", None).await?;
        assert_json_is!(messages, []);
        assert_json_is!(outputs, [[2]]);

        // The execution context should now have the var
        let (outputs, messages) = kernel.exec("ls()", None).await?;
        assert_json_is!(messages, []);
        assert_json_is!(outputs, [["a"]]);

        // Print the variable twice and then output it
        let (outputs, messages) = kernel.exec("print(a)\nprint(a)\na", None).await?;
        assert_json_is!(messages, []);
        assert_json_is!(outputs, [[2], [2], [2]]);

        // Syntax error
        let (outputs, messages) = kernel.exec("bad ^ # syntax", None).await?;
        assert_json_is!(messages[0].error_type, "SyntaxError");
        assert_json_is!(
            messages[0].error_message,
            "<text>:2:0: unexpected end of input\n1: bad ^ # syntax\n   ^"
        );
        assert_json_is!(outputs, []);

        // Runtime error
        let (outputs, messages) = kernel.exec("foo", None).await?;
        assert_json_is!(messages[0].error_type, "RuntimeError");
        assert_json_is!(messages[0].error_message, "object 'foo' not found");
        assert_json_is!(outputs, []);

        // Set and get another variable
        kernel.set("b", Node::Integer(3)).await?;
        let b = kernel.get("b").await?;
        assert_json_is!(b, 3);

        // Use both variables
        let (outputs, messages) = kernel.exec("a*b", None).await?;
        assert_json_is!(messages, []);
        assert_json_is!(outputs, [[6]]);

        // Set and get are "reversible"
        let vars: HashMap<String, Node> = [
            ("var1", Node::Boolean(true)),
            ("var2", Node::Boolean(false)),
            ("var3", Node::Integer(123)),
            ("var4", Node::Number(Number(1.23))),
            (
                "var5",
                Node::Array(Array::from([
                    Primitive::Integer(1),
                    Primitive::Integer(2),
                    Primitive::Integer(3),
                ])),
            ),
            (
                "var6",
                Node::Object(Object::from([
                    ("a".to_string(), Primitive::Integer(1)),
                    ("b".to_string(), Primitive::Integer(2)),
                    ("c".to_string(), Primitive::Integer(3)),
                ])),
            ),
        ]
        .map(|(name, node)| (name.to_string(), node))
        .into();

        for (name, value) in vars {
            kernel.set(&name, value.clone()).await?;
            let got = kernel.get(&name).await?;
            assert_json_eq!(got, value)
        }

        Ok(())
    }

    /// Test that an assignment on the last line does not generate an output
    #[tokio::test]
    async fn assignment_no_output() -> Result<()> {
        let _guard = QUEUE.lock().await;

        let mut kernel = match skip_or_kernel().await {
            Ok(kernel) => kernel,
            Err(..) => return Ok(()),
        };

        let (outputs, messages) = kernel.exec("a <- 1", None).await?;
        assert!(messages.is_empty());
        assert_json_is!(outputs, []);

        let (outputs, messages) = kernel.exec("b = 2", None).await?;
        assert!(messages.is_empty());
        assert_json_is!(outputs, []);

        let (outputs, messages) = kernel
            .exec("print(a)\nprint(b)\na_b <- a + b", None)
            .await?;
        assert!(messages.is_empty());
        assert_json_is!(outputs, [[1], [2]]);

        Ok(())
    }

    #[tokio::test]
    async fn encode_general() -> Result<()> {
        let _guard = QUEUE.lock().await;

        let mut kernel = match skip_or_kernel().await {
            Ok(kernel) => kernel,
            Err(..) => return Ok(()),
        };

        // Null, booleans, integers, numbers, strings
        let (outputs, messages) = kernel
            .exec("list(NULL, TRUE, FALSE, 1, 1.23456789, 'str')", None)
            .await?;
        assert_json_is!(messages, []);
        assert_json_is!(
            outputs,
            [[null, [true], [false], [1], [1.23456789], ["str"]]]
        );

        // Arrays
        let (outputs, messages) = kernel.exec("1:5", None).await?;
        assert_json_is!(messages, []);
        assert_json_is!(outputs, [[1, 2, 3, 4, 5]]);

        // Objects
        let (outputs, messages) = kernel.exec("list(a=1, b=list(c=2))", None).await?;
        assert_json_is!(messages, []);
        assert_json_is!(outputs, [{"a": [1], "b": {"c": [2]}}]);

        // Matrix
        let (outputs, messages) = kernel.exec("matrix(c(1:4), 2, 2)", None).await?;
        assert_json_is!(messages, []);
        assert_json_is!(outputs, [[[1, 3], [2, 4]]]);

        Ok(())
    }

    #[tokio::test]
    async fn encode_dataframes() -> Result<()> {
        let _guard = QUEUE.lock().await;

        let mut kernel = match skip_or_kernel().await {
            Ok(kernel) => kernel,
            Err(..) => return Ok(()),
        };

        let (outputs, messages) = kernel
            .exec(
                r#"data.frame(
    a = 1:2,
    b = c(TRUE, FALSE),
    c = c("x", "y"),
    d = factor(c("X", "Y"), levels = c("X", "Y", "Z")),
    stringsAsFactors = FALSE
)"#,
                None,
            )
            .await?;
        assert_json_is!(messages, []);
        let dt = match &outputs[0] {
            Node::Datatable(dt) => dt.clone(),
            _ => bail!("unexpected type {:?}", outputs[0]),
        };
        assert_eq!(
            dt.columns
                .iter()
                .map(|column| column.name.as_str())
                .collect::<Vec<&str>>(),
            vec!["a", "b", "c", "d"]
        );
        assert_json_is!(
            dt.columns[0].validator.as_ref().unwrap().items_validator,
            { "type": "NumberValidator"}
        );
        assert_json_is!(
            dt.columns[1].validator.as_ref().unwrap().items_validator,
            { "type": "BooleanValidator"}
        );
        assert_json_is!(
            dt.columns[2].validator.as_ref().unwrap().items_validator,
            { "type": "StringValidator"}
        );
        assert_json_is!(
            dt.columns[3].validator.as_ref().unwrap().items_validator,
            {
                "type": "EnumValidator",
                "values": ["X", "Y", "Z"]
            }
        );

        let (outputs, messages) = kernel.exec("mtcars", None).await?;
        assert_json_is!(messages, []);
        let dt = match &outputs[0] {
            Node::Datatable(dt) => dt.clone(),
            _ => bail!("unexpected type {:?}", outputs[0]),
        };
        assert_eq!(
            dt.columns
                .iter()
                .map(|column| column.name.as_str())
                .collect::<Vec<&str>>(),
            vec![
                "name", "mpg", "cyl", "disp", "hp", "drat", "wt", "qsec", "vs", "am", "gear",
                "carb"
            ]
        );
        assert_json_is!(
            dt.columns[0].validator.as_ref().unwrap().items_validator,
            { "type": "StringValidator"}
        );
        assert_json_is!(
            dt.columns[1].validator.as_ref().unwrap().items_validator,
            { "type": "NumberValidator"}
        );

        let (outputs, messages) = kernel.exec("chickwts", None).await?;
        assert_json_is!(messages, []);
        let dt = match &outputs[0] {
            Node::Datatable(dt) => dt.clone(),
            _ => bail!("unexpected type {:?}", outputs[0]),
        };
        assert_eq!(
            dt.columns
                .iter()
                .map(|column| column.name.as_str())
                .collect::<Vec<&str>>(),
            vec!["weight", "feed"]
        );
        assert_json_is!(
            dt.columns[0].validator.as_ref().unwrap().items_validator,
            { "type": "NumberValidator"}
        );
        assert_json_is!(
            dt.columns[1].validator.as_ref().unwrap().items_validator,
            {
                "type": "EnumValidator",
                "values": ["casein", "horsebean", "linseed", "meatmeal", "soybean", "sunflower"]
            }
        );

        Ok(())
    }

    #[tokio::test]
    async fn encode_plots() -> Result<()> {
        let _guard = QUEUE.lock().await;

        let mut kernel = match skip_or_kernel().await {
            Ok(kernel) => kernel,
            Err(..) => return Ok(()),
        };

        for code in ["plot(1)", "hist(rnorm(1000), breaks=30)"] {
            let (outputs, messages) = kernel.exec(code, None).await?;
            assert_json_is!(messages, []);
            let image = match &outputs[0] {
                Node::ImageObject(dt) => dt.clone(),
                _ => bail!("unexpected type {:?}", outputs[0]),
            };
            assert!(image.content_url.starts_with("data:image/png;base64,"));
        }

        Ok(())
    }

    /// Test interrupting a task
    #[tokio::test]
    async fn exec_async() -> Result<()> {
        let _guard = QUEUE.lock().await;

        let mut kernel = match skip_or_kernel().await {
            Ok(kernel) => {
                if kernel.is_interruptable().await {
                    kernel
                } else {
                    eprintln!("Not interruptable on this OS");
                    return Ok(());
                }
            }
            Err(..) => return Ok(()),
        };

        // Start a long running task in the kernel that should get interrupted
        let mut task = kernel
            .exec_async("started <- TRUE; Sys.sleep(10); finished <- TRUE", None)
            .await
            .unwrap();

        // Sleep a little to allow the task to start, then interrupt it
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        task.interrupt().await?;

        // Check that was started but not finished
        let (outputs, messages) = kernel
            .exec("c(started, exists('finished'))", None)
            .await
            .unwrap();
        assert_json_is!(messages, []);
        assert_json_is!(outputs, [[true, false]]);

        Ok(())
    }

    /// Test forking
    #[tokio::test]
    async fn exec_fork() -> Result<()> {
        let _guard = QUEUE.lock().await;

        let mut kernel = match skip_or_kernel().await {
            Ok(kernel) => {
                if kernel.is_forkable().await {
                    kernel
                } else {
                    eprintln!("Not forkable on this OS");
                    return Ok(());
                }
            }
            Err(..) => return Ok(()),
        };

        // In the kernel import a module and assign a variable
        let (outputs, messages) = kernel.exec("var = runif(1)\nvar", None).await?;
        assert_json_is!(messages, []);
        assert_eq!(outputs.len(), 1);
        let var = outputs[0].clone();

        // Now fork-exec. The fork should be able to use the module and access the
        // variable but any change to variable should not change its value in the parent kernel
        let mut task = kernel.exec_fork("print(var)\nvar = runif(1)", None).await?;
        let TaskResult { outputs, messages } = task.result().await?;
        assert_json_is!(messages, []);
        assert_eq!(outputs.len(), 1);
        assert_json_is!(outputs[0], var);

        // Back in the parent kernel, var should still have its original value
        let (outputs, messages) = kernel.exec("var", None).await?;
        assert_json_is!(messages, []);
        assert_eq!(outputs.len(), 1);
        assert_json_eq!(outputs[0], var);

        Ok(())
    }

    /// Test setting and getting of vars of different types
    #[tokio::test]
    async fn set_get_vars() -> Result<()> {
        let mut kernel = match skip_or_kernel().await {
            Ok(kernel) => kernel,
            Err(..) => return Ok(()),
        };

        kernel_micro::tests::set_get_strings(&mut kernel).await?;

        Ok(())
    }
}
