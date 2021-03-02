use clap::{App, Arg};
use heapquery::{
  exec_query, init_schema, insert_edges, insert_locations, insert_nodes, open_assoc_db,
  read_heap_file, setup_db_if_needed,
};
use serde_json::Value;

fn main() {
  let matches = App::new("heapquery")
    .version("0.0.1")
    .author("hsiaosiyuan0@outlook.com")
    .author("Query the objects on the heap of node.js")
    .arg(
      Arg::with_name("heap")
        .long("heap")
        .required(true)
        .value_name("heap")
        .help("The heap file produced from `v8.getHeapSnapshot`")
        .takes_value(true),
    )
    .arg(
      Arg::with_name("query")
        .long("query")
        .value_name("query")
        .help("The SQL to query your data")
        .takes_value(true),
    )
    .get_matches();

  let heap_file = matches.value_of("heap").unwrap();
  if setup_db_if_needed(heap_file) {
    let heap_json: Value = read_heap_file(heap_file);
    let mut conn = open_assoc_db(heap_file);
    init_schema(&conn);
    insert_nodes(&heap_json, &mut conn);
    insert_edges(&heap_json, &mut conn);
    insert_locations(&heap_json, &mut conn);
  }

  if let Some(query) = matches.value_of("query") {
    let conn = open_assoc_db(heap_file);
    exec_query(&conn, query)
  }
}
