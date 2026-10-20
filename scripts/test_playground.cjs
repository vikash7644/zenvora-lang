const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");

const html = fs.readFileSync("website/playground.html", "utf8");
const scripts = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)];
assert.equal(scripts.length, 1, "playground should have exactly one inline script");
assert.match(html, /data-example="functions"/, "functions example button exists");
assert.match(html, /data-example="collections"/, "collections example button exists");
assert.match(html, /data-example="stdlib"/, "standard library example button exists");

class FakeElement {
  constructor(dataset = {}) {
    this.dataset = dataset;
    this.listeners = {};
    this.style = {};
    this.value = "";
    this.textContent = "";
    this.innerHTML = "";
    this.className = "";
  }
  addEventListener(name, callback) { this.listeners[name] = callback; }
  click() { assert.ok(this.listeners.click, "expected click handler"); return this.listeners.click(); }
}

const ids = ["code", "output", "status", "run", "reset", "copy"];
const elements = Object.fromEntries(ids.map(id => [id, new FakeElement()]));
const editorMatch = html.match(/<textarea id="code"[^>]*>([\s\S]*?)<\/textarea>/);
assert.ok(editorMatch, "playground editor exists");
elements.code.value = editorMatch[1];
const exampleNames = ["variables", "math", "booleans", "condition", "loops", "functions", "collections", "stdlib", "combined"];
const examples = exampleNames.map(name => new FakeElement({ example: name }));
const document = {
  getElementById(id) { assert.ok(elements[id], "unexpected element id: " + id); return elements[id]; },
  querySelectorAll(selector) { assert.equal(selector, "[data-example]"); return examples; }
};
const context = { document, navigator: { clipboard: { writeText: async () => {} } }, console };
vm.runInNewContext(scripts[0][1], context);

function run(source, expected, label) {
  elements.code.value = source;
  elements.run.click();
  assert.equal(elements.status.textContent, "SUCCESS", label + " should succeed: " + elements.output.textContent);
  assert.equal(elements.output.textContent, expected, label);
}

run(elements.code.value, "30\n20", "default arithmetic example");
run('define count := 2\ncount := count + 3\nshow count', "5", "variable assignment");
run('define name := "Zenvora"\nshow "Hello, " + name', "Hello, Zenvora", "strings and concatenation");
run('if true {\n  show "yes"\n} otherwise {\n  show "no"\n}', "yes", "if branch");
run('if false {\n  show "yes"\n} otherwise {\n  show "no"\n}', "no", "otherwise branch");
run('define count := 0\nrepeat count < 5 {\n count := count + 1\n if count == 2 {\n  continue\n }\n show count\n if count == 4 {\n  break\n }\n}', "1\n3\n4", "repeat, break, and continue");
run('show multiply(6, 7)\nfn multiply(a, b) {\n return a * b\n}', "42", "function declaration and return");
run('define items := array(10, 20, 30)\nshow get(items, 1)\nshow len(items)\ndefine user := map("name", "Zenvora", "level", 8)\nshow get(user, "name")\nshow keys(user)', "20\n3\nZenvora\n[level, name]", "arrays and maps");
run('show type_of(42)\nshow to_string(true)\nshow abs(-15)\nshow min(7, 3)\nshow range(1, 4)', "integer\ntrue\n15\n3\n[1, 2, 3]", "standard library helpers");

elements.code.value = "show 1 / 0";
elements.run.click();
assert.match(elements.output.textContent, /Division by zero/, "division-by-zero error is reported");
elements.code.value = "show missing_name";
elements.run.click();
assert.match(elements.output.textContent, /Undefined variable/, "undefined names are reported");

examples.find(button => button.dataset.example === "functions").click();
assert.ok(elements.code.value.includes("fn multiply"), "functions example loads");
examples.find(button => button.dataset.example === "collections").click();
assert.ok(elements.code.value.includes("map("), "collections example loads");
examples.find(button => button.dataset.example === "stdlib").click();
assert.ok(elements.code.value.includes("range("), "standard-library example loads");
examples.find(button => button.dataset.example === "combined").click();
run(elements.code.value, "Building with Zenvora\n3\n1\nBuild step\n3\n42", "combined demo with nested otherwise blocks");
run('show factorial(5)\nfn factorial(n) {\n if n <= 1 {\n  return 1\n } otherwise {\n  return n * factorial(n - 1)\n }\n}', "120", "recursive function calls and return values");

console.log("Playground tests passed: expressions, branching, nested otherwise blocks, loops, recursive functions, collections, standard library, and error handling.");
