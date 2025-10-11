//! Audit logging macros for automatic function and data tracing

/// Macro for auditing function entry
///
/// Usage:
/// ```
/// audit_fn_entry!();
/// audit_fn_entry!(param1 = value1, param2 = value2);
/// ```
#[macro_export]
macro_rules! audit_fn_entry {
    () => {
        if $crate::audit::is_audit_enabled() {
            let entry = $crate::audit::audit_entry(
                module_path!(),
                &format!("{}", std::any::type_name_of_val(&||{}))
                    .split("::")
                    .last()
                    .unwrap_or("unknown")
                    .replace("{{closure}}", ""),
                file!(),
                line!(),
                serde_json::json!({}),
            );
            $crate::audit::write_audit(entry);
        }
    };
    ($($key:ident = $value:expr),* $(,)?) => {
        if $crate::audit::is_audit_enabled() {
            let mut data = serde_json::Map::new();
            $(
                data.insert(
                    stringify!($key).to_string(),
                    serde_json::to_value(&$value).unwrap_or(serde_json::Value::Null)
                );
            )*
            let entry = $crate::audit::audit_entry(
                module_path!(),
                &format!("{}", std::any::type_name_of_val(&||{}))
                    .split("::")
                    .last()
                    .unwrap_or("unknown")
                    .replace("{{closure}}", ""),
                file!(),
                line!(),
                serde_json::Value::Object(data),
            );
            $crate::audit::write_audit(entry);
        }
    };
}

/// Macro for auditing function exit
///
/// Usage:
/// ```
/// audit_fn_exit!();
/// audit_fn_exit!(result = value);
/// audit_fn_exit!(result = value, duration = elapsed);
/// ```
#[macro_export]
macro_rules! audit_fn_exit {
    () => {
        if $crate::audit::is_audit_enabled() {
            let entry = $crate::audit::audit_exit(
                module_path!(),
                &format!("{}", std::any::type_name_of_val(&||{}))
                    .split("::")
                    .last()
                    .unwrap_or("unknown")
                    .replace("{{closure}}", ""),
                file!(),
                line!(),
                serde_json::json!({}),
                None,
            );
            $crate::audit::write_audit(entry);
        }
    };
    ($($key:ident = $value:expr),* $(,)?) => {
        if $crate::audit::is_audit_enabled() {
            let mut data = serde_json::Map::new();
            let mut duration = None;
            $(
                if stringify!($key) == "duration" {
                    duration = Some($value);
                } else {
                    data.insert(
                        stringify!($key).to_string(),
                        serde_json::to_value(&$value).unwrap_or(serde_json::Value::Null)
                    );
                }
            )*
            let entry = $crate::audit::audit_exit(
                module_path!(),
                &format!("{}", std::any::type_name_of_val(&||{}))
                    .split("::")
                    .last()
                    .unwrap_or("unknown")
                    .replace("{{closure}}", ""),
                file!(),
                line!(),
                serde_json::Value::Object(data),
                duration,
            );
            $crate::audit::write_audit(entry);
        }
    };
}

/// Macro for auditing data transformations
///
/// Usage:
/// ```
/// audit_data!("Parsing FASTA sequences", count = 1000, size_mb = 45.2);
/// ```
#[macro_export]
macro_rules! audit_data {
    ($description:expr $(, $key:ident = $value:expr)* $(,)?) => {
        if $crate::audit::is_audit_enabled() {
            let mut data = serde_json::Map::new();
            $(
                data.insert(
                    stringify!($key).to_string(),
                    serde_json::to_value(&$value).unwrap_or(serde_json::Value::Null)
                );
            )*
            let entry = $crate::audit::audit_data(
                module_path!(),
                &format!("{}", std::any::type_name_of_val(&||{}))
                    .split("::")
                    .last()
                    .unwrap_or("unknown")
                    .replace("{{closure}}", ""),
                file!(),
                line!(),
                $description,
                serde_json::Value::Object(data),
            );
            $crate::audit::write_audit(entry);
        }
    };
}

/// Macro for auditing algorithm execution
///
/// Usage:
/// ```
/// audit_algo!("reference_selection", sequences = 1000, references = 100);
/// ```
#[macro_export]
macro_rules! audit_algo {
    ($algorithm:expr $(, $key:ident = $value:expr)* $(,)?) => {
        if $crate::audit::is_audit_enabled() {
            let mut data = serde_json::Map::new();
            $(
                data.insert(
                    stringify!($key).to_string(),
                    serde_json::to_value(&$value).unwrap_or(serde_json::Value::Null)
                );
            )*
            let entry = $crate::audit::audit_algorithm(
                module_path!(),
                &format!("{}", std::any::type_name_of_val(&||{}))
                    .split("::")
                    .last()
                    .unwrap_or("unknown")
                    .replace("{{closure}}", ""),
                file!(),
                line!(),
                $algorithm,
                serde_json::Value::Object(data),
            );
            $crate::audit::write_audit(entry);
        }
    };
}

/// Macro for creating an audited function wrapper
///
/// Usage:
/// ```
/// audit_fn! {
///     fn process_sequences(sequences: Vec<Sequence>) -> Result<()> {
///         // function body
///     }
/// }
/// ```
#[macro_export]
macro_rules! audit_fn {
    (
        $(#[$attr:meta])*
        $vis:vis fn $name:ident $(<$($generic:tt),*>)? (
            $($arg_name:ident : $arg_type:ty),* $(,)?
        ) $(-> $ret:ty)? {
            $($body:tt)*
        }
    ) => {
        $(#[$attr])*
        $vis fn $name $(<$($generic),*>)? (
            $($arg_name : $arg_type),*
        ) $(-> $ret)? {
            let _audit_start = std::time::Instant::now();

            // Log function entry with parameter info (excluding large data)
            if $crate::audit::is_audit_enabled() {
                let mut _audit_params = serde_json::Map::new();
                $(
                    // Only log simple types and sizes of collections
                    let _param_value = if std::mem::size_of_val(&$arg_name) > 1024 {
                        serde_json::json!({
                            "type": stringify!($arg_type),
                            "size": "large"
                        })
                    } else {
                        serde_json::to_value(&$arg_name).unwrap_or_else(|_|
                            serde_json::json!({
                                "type": stringify!($arg_type),
                                "value": "non-serializable"
                            })
                        )
                    };
                    _audit_params.insert(stringify!($arg_name).to_string(), _param_value);
                )*

                let entry = $crate::audit::audit_entry(
                    module_path!(),
                    stringify!($name),
                    file!(),
                    line!(),
                    serde_json::Value::Object(_audit_params),
                );
                $crate::audit::write_audit(entry);
            }

            // Execute function body
            let _audit_result = (|| $(-> $ret)? {
                $($body)*
            })();

            // Log function exit with duration
            if $crate::audit::is_audit_enabled() {
                let entry = $crate::audit::audit_exit(
                    module_path!(),
                    stringify!($name),
                    file!(),
                    line!(),
                    serde_json::json!({
                        "success": true
                    }),
                    Some(_audit_start.elapsed()),
                );
                $crate::audit::write_audit(entry);
            }

            _audit_result
        }
    };
}
