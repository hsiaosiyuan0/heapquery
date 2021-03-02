use core::fmt;
use rusqlite::{params, types::ValueRef, Connection};
use serde_json::Value;
use std::{collections::HashMap, format, fs, path::Path};

pub fn assoc_db_name(heap_file: &str) -> String {
  let path = Path::new(heap_file);
  format!("{}.db3", path.file_stem().unwrap().to_str().unwrap())
}

pub fn setup_db_if_needed(heap_file: &str) -> bool {
  !Path::new(heap_file).exists()
}

pub fn open_db(path: &str) -> Connection {
  Connection::open(&path).expect("unable to open db")
}

pub fn open_assoc_db(heap_file: &str) -> Connection {
  let db_name = assoc_db_name(heap_file);
  open_db(&db_name)
}

pub fn read_heap_file(heap_file: &str) -> Value {
  let heap_data = fs::read_to_string(heap_file).expect("unable to open file");
  serde_json::from_str(heap_data.as_str()).expect("deformed heap file")
}

pub fn init_schema(conn: &Connection) {
  conn
    .execute_batch(
      "
    CREATE TABLE IF NOT EXISTS node (
      id INTEGER PRIMARY KEY,
      name VARCHAR(50),
      type VARCHAR(50),
      self_size INTEGER,
      children_count INTEGER
    );
    
    CREATE TABLE IF NOT EXISTS edge (
      `from` INTEGER,
      `to` INTEGER,
      type VARCHAR(50),
      name_or_index VARCHAR(50)
    );

    CREATE TABLE IF NOT EXISTS location (
      node_id INTEGER,
      script_id INTEGER,
      line INTEGER,
      col INTEGER
    );
    ",
    )
    .expect("unable to init schema");
}

pub fn insert_nodes(heap_json: &Value, conn: &mut Connection) {
  let meta = &heap_json["snapshot"]["meta"];

  let node_fields = meta["node_fields"].as_array().unwrap();
  let node_field_types = meta["node_types"].as_array().unwrap();

  let strings = heap_json["strings"].as_array().unwrap();

  let node_field_values = heap_json["nodes"].as_array().unwrap();
  let node_field_values_len = node_field_values.len();

  let mut i = 0;

  let tx = conn.transaction().unwrap();

  while i < node_field_values_len {
    let mut node = HashMap::new();

    node_fields.into_iter().enumerate().for_each(|item| {
      let key = item.1.as_str().unwrap();
      let value_type = &node_field_types[item.0];
      let maybe_value = &node_field_values[i];

      let value = if value_type.is_array() {
        let enum_values = value_type.as_array().unwrap();
        &enum_values[maybe_value.as_u64().unwrap() as usize]
      } else if value_type.as_str().unwrap() == "string" {
        &strings[maybe_value.as_u64().unwrap() as usize]
      } else if value_type.as_str().unwrap() == "number" {
        maybe_value
      } else {
        unreachable!(format!("unsupported type: {}", value_type));
      };

      node.insert(key, value);
      i += 1;
    });

    tx.execute(
      "
    INSERT INTO node (id, name, type, self_size, children_count)
    VALUES (?1, ?2, ?3, ?4, ?5)
    ",
      params![
        node["id"].as_u64().unwrap() as u32,
        node["name"].as_str().unwrap(),
        node["type"].as_str().unwrap(),
        node["self_size"].as_u64().unwrap() as u32,
        node["edge_count"].as_u64().unwrap() as u32
      ],
    )
    .expect("failed to insert node");
  }

  tx.commit().expect("failed to commit");
}

pub fn insert_edges(heap_json: &Value, conn: &mut Connection) {
  let meta = &heap_json["snapshot"]["meta"];
  let node_fields = meta["node_fields"].as_array().unwrap();
  let node_fields_len = node_fields.len();
  let strings = heap_json["strings"].as_array().unwrap();

  let node_field_values = heap_json["nodes"].as_array().unwrap();
  let node_field_values_len = node_field_values.len();

  let edge_fields = meta["edge_fields"].as_array().unwrap();
  let edge_field_types = meta["edge_types"].as_array().unwrap();
  let edge_field_values = heap_json["edges"].as_array().unwrap();

  // below values are noticeable to keep sync with the fields order in heapsnapshot
  let node_id_ofst = 2;
  let edge_count_ofst = 4;

  let mut node_i = 0;
  let mut edge_i = 0;

  let tx = conn.transaction().unwrap();

  while node_i < node_field_values_len {
    let node_id = node_field_values[node_i + node_id_ofst].as_u64().unwrap();
    let edge_count = node_field_values[node_i + edge_count_ofst]
      .as_u64()
      .unwrap();

    for _ in 0..edge_count {
      let mut edge = HashMap::new();
      edge_fields.into_iter().enumerate().for_each(|item| {
        let key = item.1.as_str().unwrap();
        let value_type = &edge_field_types[item.0];
        let maybe_value = &edge_field_values[edge_i];

        let value = if value_type.is_array() {
          let enum_values = value_type.as_array().unwrap();
          &enum_values[maybe_value.as_u64().unwrap() as usize]
        } else if value_type.as_str().unwrap() == "string"
          || value_type.as_str().unwrap() == "string_or_number"
        {
          &strings[maybe_value.as_u64().unwrap() as usize]
        } else if value_type.as_str().unwrap() == "number" || value_type.as_str().unwrap() == "node"
        {
          maybe_value
        } else {
          unreachable!(format!("unsupported type: {}", value_type));
        };

        edge.insert(key, value);

        edge_i += 1;
      });

      let to_node_index = edge["to_node"].as_u64().unwrap();
      let to_node_id = node_field_values[(to_node_index as usize) + node_id_ofst]
        .as_u64()
        .unwrap() as u32;

      tx.execute(
        "
      INSERT INTO edge (`from`, `to`, type, name_or_index)
      VALUES (?1, ?2, ?3, ?4)
      ",
        params![
          node_id as u32,
          to_node_id,
          edge["type"].as_str().unwrap(),
          if edge["name_or_index"].is_number() {
            edge["name_or_index"].as_u64().unwrap().to_string()
          } else {
            edge["name_or_index"].as_str().unwrap().to_string()
          },
        ],
      )
      .expect("failed to insert node");
    }

    node_i += node_fields_len;
  }

  tx.commit().expect("failed to commit");
}

pub fn insert_locations(heap_json: &Value, conn: &mut Connection) {
  let loc_field_values = heap_json["locations"].as_array().unwrap();
  let loc_field_values_len = loc_field_values.len();

  let node_field_values = heap_json["nodes"].as_array().unwrap();

  // below values are noticeable to keep sync with the fields order in heapsnapshot
  let node_id_ofst = 2;

  let tx = conn.transaction().unwrap();

  let mut i = 0;
  while i < loc_field_values_len {
    let object_index = loc_field_values[i].as_u64().unwrap() as usize;
    let node_id = node_field_values[object_index + node_id_ofst]
      .as_u64()
      .unwrap();
    i += 1;

    let script_id = loc_field_values[i].as_u64().unwrap();
    i += 1;

    let line = loc_field_values[i].as_u64().unwrap();
    i += 1;

    let col = loc_field_values[i].as_u64().unwrap();
    i += 1;

    tx.execute(
      "
    INSERT INTO location (node_id, script_id, line, col)
    VALUES (?1, ?2, ?3, ?4)
    ",
      params![node_id as u32, script_id as u32, line as u32, col as u32,],
    )
    .expect("failed to insert node");
  }

  tx.commit().expect("failed to commit");
}

pub enum ColumnValue {
  Integer(i64),
  Real(f64),
  Text(String),
  Null,
}

impl fmt::Debug for ColumnValue {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      ColumnValue::Integer(i) => write!(f, "{}", i),
      ColumnValue::Real(r) => write!(f, "{}", r),
      ColumnValue::Text(t) => write!(f, "{}", t),
      ColumnValue::Null => write!(f, "{}", "null"),
    }
  }
}

pub fn exec_query(conn: &Connection, sql: &str) {
  println!("run sql: {}", sql);

  let mut stmt = conn.prepare(sql).expect("failed to prepare query");
  let rows = stmt
    .query_map(params![], |row| {
      let mut ret = HashMap::new();
      row.column_names().into_iter().enumerate().for_each(|item| {
        let v = row.get_raw_checked(item.0).unwrap();
        let cv = match v {
          ValueRef::Integer(i) => ColumnValue::Integer(i),
          ValueRef::Real(r) => ColumnValue::Real(r),
          ValueRef::Text(t) => ColumnValue::Text(String::from_utf8(t.to_owned()).unwrap()),
          ValueRef::Null => ColumnValue::Null,
          ValueRef::Blob(_) => unimplemented!("unsupported value type: Blob"),
        };
        ret.insert(item.1.to_string(), cv);
      });
      Ok(ret)
    })
    .expect("failed to run query");

  for r in rows {
    println!("{:?}", r.unwrap());
  }
}
