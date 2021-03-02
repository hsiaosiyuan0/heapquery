# Heapquery

Importing the output of [v8.getHeapSnapshot](https://nodejs.org/api/v8.html#v8_v8_getheapsnapshot) into sqlite.

## Usage

```
cargo run -- --heap path_to_your_heapsnapshot.heapsnapshot \
             --query 'select * from node where name="HugeObj"'
```

Above command will produce a database file with name `path_to_your_heapsnapshot.db3`, you can also use other sqlite browser to open it.

For how to produce a `.heapsnapshot` file, save and run below code to quickly get one:

```js
const { writeHeapSnapshot } = require("v8");

class HugeObj {
  constructor() {
    this.hugeData = Buffer.alloc((1 << 20) * 50, 0);
  }
}

module.exports.data = new HugeObj();

writeHeapSnapshot();
```