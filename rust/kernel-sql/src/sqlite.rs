use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use sqlx::{sqlite::SqliteArguments, Arguments, Column, Row, SqlitePool, TypeInfo};

use kernel::{
    common::{
        eyre::Result,
        itertools::Itertools,
        regex::Captures,
        serde_json,
        tokio::{self, sync::mpsc, time},
        tracing,
    },
    graph_triples::{
        resources::{self, ResourceChangeAction},
        ResourceChange,
    },
    stencila_schema::{
        ArrayValidator, BooleanValidator, Datatable, DatatableColumn, Date, IntegerValidator, Node,
        Null, Number, NumberValidator, StringValidator, ValidatorTypes,
    },
};

use crate::{WatchedTables, BINDING_REGEX};

/// Bind parameters to an SQL statement based on name
fn bind<'lt>(sql: &str, parameters: &'lt HashMap<String, Node>) -> (String, SqliteArguments<'lt>) {
    let mut count = 0;
    let mut arguments = SqliteArguments::default();
    let sql = BINDING_REGEX.replace_all(sql, |captures: &Captures| {
        let name = captures[1].to_string();
        let value = parameters.get(&name).unwrap();
        match value {
            Node::Boolean(value) => arguments.add(value),
            Node::Integer(value) => arguments.add(value),
            Node::Number(value) => arguments.add(value.0),
            Node::String(value) => arguments.add(value),
            _ => arguments.add(serde_json::to_value(&value).unwrap_or(serde_json::Value::Null)),
        };
        count += 1;
        ["?", &count.to_string()].concat()
    });
    (sql.to_string(), arguments)
}

/// Execute an SQL statement in SQLite
///
/// Only returns a `Datatable` for convenience elsewhere in the code
pub async fn execute_statement(
    sql: &str,
    parameters: &HashMap<String, Node>,
    pool: &SqlitePool,
) -> Result<Datatable> {
    let (sql, args) = bind(sql, parameters);
    sqlx::query_with(&sql, args).execute(pool).await?;
    Ok(Datatable::default())
}

/// Run a query in SQLite and return the result as a Stencila [`Datatable`]
pub async fn query_to_datatable(
    query: &str,
    parameters: &HashMap<String, Node>,
    pool: &SqlitePool,
) -> Result<Datatable> {
    // Run the query
    let (sql, args) = bind(query, parameters);
    let rows = sqlx::query_with(&sql, args).fetch_all(pool).await?;

    // Get the names of the columns and transform their types into validators
    let columns = if let Some(row) = rows.first() {
        row.columns()
            .iter()
            .map(|column| {
                let name = column.name().to_string();
                let col_type = column.type_info().name().to_string();
                let validator = match col_type.as_str() {
                    "BOOLEAN" => {
                        Some(ValidatorTypes::BooleanValidator(BooleanValidator::default()))
                    }
                    "INTEGER" => {
                        Some(ValidatorTypes::IntegerValidator(IntegerValidator::default()))
                    }
                    "REAL" => Some(ValidatorTypes::NumberValidator(NumberValidator::default())),
                    "TEXT" => Some(ValidatorTypes::StringValidator(StringValidator::default())),
                    "NULL" => None, // No column type specified e.g. "SELECT 1;"
                    _ => {
                        tracing::debug!(
                            "Unhandled column type, will have no validator: {}",
                            col_type
                        );
                        None
                    }
                };
                (name, col_type, validator)
            })
            .collect()
    } else {
        Vec::new()
    };

    // Pre-allocate an vector of the size needed to hold all values and insert them in
    // column-first order
    let rows_len = rows.len();
    let mut values: Vec<Node> = vec![Node::Null(Null {}); columns.len() * rows_len];
    for (row_index, row) in rows.into_iter().enumerate() {
        for (col_index, (_name, col_type, ..)) in columns.iter().enumerate() {
            let position = col_index * rows_len + row_index;
            let value = match col_type.as_str() {
                "BOOLEAN" => row
                    .try_get::<bool, usize>(col_index)
                    .map(Node::Boolean)
                    .ok(),
                "INTEGER" => row.try_get::<i64, usize>(col_index).map(Node::Integer).ok(),
                "REAL" => row
                    .try_get::<f64, usize>(col_index)
                    .map(|num| Node::Number(Number(num)))
                    .ok(),
                "TEXT" => row
                    .try_get::<String, usize>(col_index)
                    .map(Node::String)
                    .ok(),
                "DATETIME" => row
                    .try_get::<String, usize>(col_index)
                    .map(|date| Node::Date(Date::from(date)))
                    .ok(),
                _ => row
                    .try_get_unchecked::<String, usize>(col_index)
                    .ok()
                    .and_then(|json| serde_json::from_str(&json).ok()),
            };
            if let Some(value) = value {
                values[position] = value;
            }
        }
    }

    // Create datatable
    let columns = columns
        .into_iter()
        .map(|(name, _col_type, validator)| DatatableColumn {
            name,
            validator: validator.map(|validator| {
                Box::new(ArrayValidator {
                    items_validator: Some(Box::new(validator)),
                    ..Default::default()
                })
            }),
            values: values.drain(..rows_len).collect(),
            ..Default::default()
        })
        .collect();
    Ok(Datatable {
        columns,
        ..Default::default()
    })
}

/// Create a SQLite table from a Stencila [`Datatable`]
pub async fn table_from_datatable(
    name: &str,
    datatable: Datatable,
    pool: &SqlitePool,
) -> Result<()> {
    sqlx::query(&format!("DROP TABLE IF EXISTS \"{}\"", name))
        .execute(pool)
        .await?;

    let columns = datatable
        .columns
        .iter()
        .map(|column| {
            let validator = column
                .validator
                .as_deref()
                .and_then(|array_validator| array_validator.items_validator.clone());
            let datatype = match validator.as_deref() {
                Some(ValidatorTypes::BooleanValidator(..)) => "BOOLEAN",
                Some(ValidatorTypes::IntegerValidator(..)) => "INTEGER",
                Some(ValidatorTypes::NumberValidator(..)) => "REAL",
                Some(ValidatorTypes::StringValidator(..)) => "TEXT",
                _ => "JSON",
            };
            format!("{} {}", column.name, datatype)
        })
        .collect_vec()
        .join(", ");
    sqlx::query(&format!("CREATE TABLE \"{}\"({});\n", name, columns))
        .execute(pool)
        .await?;

    let rows = datatable
        .columns
        .first()
        .map(|column| column.values.len())
        .unwrap_or(0);
    if rows == 0 {
        return Ok(());
    }

    let cols = datatable.columns.len();

    let mut sql = format!("INSERT INTO \"{}\" VALUES\n", name);
    sql += &vec![format!(" ({})", vec!["?"; cols].join(", ")); rows].join(",\n");

    let mut query = sqlx::query(&sql);
    for row in 0..rows {
        for col in 0..cols {
            let column = &datatable.columns[col];
            let node = &column.values[row];
            match node {
                Node::Null(..) => query = query.bind("null"),
                Node::Boolean(value) => query = query.bind(value),
                Node::Integer(value) => query = query.bind(value),
                Node::Number(value) => query = query.bind(value.0),
                Node::String(value) => query = query.bind(value),
                _ => query = query.bind(serde_json::to_string(node).unwrap_or_default()),
            }
        }
    }
    query.execute(pool).await?;

    Ok(())
}

/**
 * Start a background task to listen for notifications of changes to tables
 *
 * SQLite does have ["Data Change Notification Callbacks"](https://www.sqlite.org/c3ref/update_hook.html)
 * and `rusqlite` crate has a `update_hook` method. However, these hooks only receive events (inserts, updates, deletes)
 * that were made by the same connection, not other connections or other database.
 * See the discussion at https://sqlite.org/forum/info/b77046785208132f.
 *
 * Given that, this implements a polling approach which listens for changes in a hidden
 * notifications table. At present it is somewhat rudimentary but allows for testing
 * of other logic around table watches.
 */
pub async fn watch(
    url: &str,
    pool: &SqlitePool,
    watches: WatchedTables,
    sender: mpsc::Sender<ResourceChange>,
) -> Result<()> {
    // Create table for recording changes and a trigger to purge events older than 60s
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS stencila_resource_changes(
            "time" INTEGER,
            "action" TEXT,
            "table" TEXT
        );

        CREATE TRIGGER IF NOT EXISTS stencila_resource_changes_purge
        AFTER INSERT ON stencila_resource_changes
        BEGIN
            DELETE FROM stencila_resource_changes
            WHERE time < (julianday('now') - 2440587.5) * 86400000 - (60 * 1000);
        END;
        "#,
    )
    .execute(pool)
    .await?;

    let url = url.to_string();
    let pool = pool.clone();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(300));
        let mut last_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        loop {
            interval.tick().await;

            let watches = watches.read().await;
            if watches.is_empty() {
                continue;
            }

            let rows = match sqlx::query(
                r#"
                SELECT "time", "action", "table"
                FROM stencila_resource_changes
                WHERE "time" > ?
                GROUP BY time, action, "table"
                ORDER BY time;
                "#,
            )
            .bind(last_time as i64)
            .fetch_all(&pool)
            .await
            {
                Ok(row) => row,
                Err(error) => {
                    tracing::error!("While polling for SQLite notifications: {}", error);
                    continue;
                }
            };

            let path = PathBuf::from(url.clone()).join("public");

            for row in rows {
                let name = row.get_unchecked::<String, _>("table");
                let time = row.get_unchecked::<i64, _>("time");

                if !watches.contains(&name) {
                    continue;
                }

                let change = ResourceChange {
                    resource: resources::symbol(&path, &name, "Datatable"),
                    action: ResourceChangeAction::Updated,
                    time: time.to_string(),
                };
                if let Err(error) = sender.send(change).await {
                    tracing::error!(
                        "While sending resource change from SQLite listener: {}",
                        error
                    );
                }

                last_time = time as u128;
            }
        }
    });

    Ok(())
}

/// Set up watches for a particular table
pub async fn watch_table(table: &str, pool: &SqlitePool) -> Result<()> {
    for action in ["insert", "update", "delete"] {
        sqlx::query(&format!(
            r#"
            CREATE TRIGGER IF NOT EXISTS stencila_resource_{action}_{table}
            AFTER {action} ON "{table}"
            BEGIN
                INSERT INTO stencila_resource_changes("time", "action", "table")
                VALUES(
                    CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER),
                    '{action}',
                    '{table}'
                );
            END;
            "#
        ))
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Set up watches for `@all` tables
pub async fn watch_all(schema: Option<&String>, pool: &SqlitePool) -> Result<Vec<String>> {
    let schema = schema.map_or_else(|| "main".to_string(), String::from);

    let tables = sqlx::query(&format!(
        r#"
        SELECT "name" FROM "{schema}"."sqlite_master"
        WHERE "type" = 'table' AND "name" != 'stencila_resource_changes'
        "#
    ))
    .fetch_all(pool)
    .await?;

    let mut names = Vec::with_capacity(tables.len());
    for table in tables {
        let name = table.get_unchecked("name");
        watch_table(name, pool).await?;
        names.push(name.to_owned());
    }

    Ok(names)
}
