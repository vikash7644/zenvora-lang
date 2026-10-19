const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");

const html = fs.readFileSync("website/playground.html", "utf8");
const scripts = [...html.matchAll(/<script>([\\s\\S]*?)<\\/script>/g)];
assert.equal(scripts.length, 1, "playground should have exactly one inline script");

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
  addEventListener(name, callback) {
    this.listeners[name] = callback;
  }
  click() {
    assert.ok(this.listeners.click, "expected click handler");
    return this.listeners.click();
  }
}

const ids = ["code", "output", "status", "run", "reset", "copy"];
const elements = Object.fromEntries(ids.map(id => [id, new FakeElement()]));
elements.code.value = html.match(/<textarea id="code"[^>]*>([\\s\\S]*?)<\\/textarea>/)[1];
const examples = ["variables", "math", "booleans"].map(name => new FakeElement({ example: name }));
const document = {
  getElementById(id) {
    assert.ok(elements[id], "unexpected element id: " + id);
    return elements[id];
  },
  querySelectorAll(selector) {
    assert.equal(selector, "[data-example]");
    return examples;
  }
};
const context = {
  document,
  navigator: { clipboard: { writeText: async () => {} } },
  console,
};
vm.runInNewContext(scripts[0][1], context);

elements.run.click();
assert.equal(elements.output.textContent, "30\\n20", "default arithmetic example");
assert.equal(elements.status.textContent, "SUCCESS", "successful program status");

elements.code.value = "define count := 2\\ncount := count + 3\\nshow count";
elements.run.click();
assert.equal(elements.output.textContent, "5", "variable assignment");

elements.code.value = "show 1 / 0";
elements.run.click();
assert.match(elements.output.textContent, /Division by zero/, "division-by-zero error");

elements.code.value = "if true { show 1 }";
elements.run.click();
assert.match(elements.output.textContent, /unsupported statement/, "unsupported syntax should fail clearly");

examples.find(button => button.dataset.example === "math").click();
assert.match(elements.code.value, /show \\(a \\+ b\\) \/ 3/, "math example loads");

console.log("Playground smoke tests passed (5 assertions groups).");
