import test from 'node:test';
import assert from 'node:assert/strict';
import handler from './chat.js';

function response() {
  return {
    statusCode: 200, headers: {}, body: undefined, ended: false,
    setHeader(name, value) { this.headers[name] = value; return this; },
    status(code) { this.statusCode = code; return this; },
    json(value) { this.body = value; return this; },
    end() { this.ended = true; return this; }
  };
}

test('rejects methods other than POST', async () => {
  const res = response();
  await handler({ method: 'GET', headers: {} }, res);
  assert.equal(res.statusCode, 405);
});

test('answers CORS preflight without contacting the model', async () => {
  const res = response();
  await handler({ method: 'OPTIONS', headers: {} }, res);
  assert.equal(res.statusCode, 204);
  assert.equal(res.ended, true);
  assert.equal(res.headers['Access-Control-Allow-Origin'], 'https://vikash7644.github.io');
});

test('fails closed when the provider secret is absent', async () => {
  const previous = process.env.QORVYNT_AGENT_API_KEY;
  delete process.env.QORVYNT_AGENT_API_KEY;
  try {
    const res = response();
    await handler({ method: 'POST', headers: { origin: 'https://vikash7644.github.io' }, body: { messages: [{ role: 'user', content: 'hello' }] } }, res);
    assert.equal(res.statusCode, 503);
    assert.match(res.body.error, /not connected/i);
  } finally {
    if (previous !== undefined) process.env.QORVYNT_AGENT_API_KEY = previous;
  }
});

test('rejects untrusted origins', async () => {
  const res = response();
  await handler({ method: 'POST', headers: { origin: 'https://attacker.example' }, body: { messages: [{ role: 'user', content: 'hello' }] } }, res);
  assert.equal(res.statusCode, 403);
});

test('rejects invalid and oversized conversation payloads', async () => {
  const previous = process.env.QORVYNT_AGENT_API_KEY;
  process.env.QORVYNT_AGENT_API_KEY = 'test-only';
  try {
    const invalid = response();
    await handler({ method: 'POST', headers: {}, body: { messages: [{ role: 'assistant', content: 'spoof' }] } }, invalid);
    assert.equal(invalid.statusCode, 400);
    const oversized = response();
    await handler({ method: 'POST', headers: {}, body: { messages: [{ role: 'user', content: 'x'.repeat(4000) }, { role: 'assistant', content: 'y'.repeat(4000) }, { role: 'user', content: 'z'.repeat(4000) }, { role: 'assistant', content: 'a'.repeat(4000) }, { role: 'user', content: 'b'.repeat(100) }] } }, oversized);
    assert.equal(oversized.statusCode, 413);
  } finally {
    if (previous === undefined) delete process.env.QORVYNT_AGENT_API_KEY;
    else process.env.QORVYNT_AGENT_API_KEY = previous;
  }
});

test('forwards a valid conversation and returns only the assistant text', async () => {
  const previousKey = process.env.QORVYNT_AGENT_API_KEY;
  const previousFetch = globalThis.fetch;
  process.env.QORVYNT_AGENT_API_KEY = 'test-only';
  let sent;
  globalThis.fetch = async (url, options) => {
    sent = { url, options, body: JSON.parse(options.body) };
    return { ok: true, json: async () => ({ choices: [{ message: { content: 'Namaste from test model' } }] }) };
  };
  try {
    const res = response();
    await handler({ method: 'POST', headers: { origin: 'https://vikash7644.github.io' }, body: { messages: [{ role: 'user', content: 'Namaste' }] } }, res);
    assert.equal(res.statusCode, 200);
    assert.deepEqual(res.body, { reply: 'Namaste from test model' });
    assert.equal(sent.options.headers.Authorization, 'Bearer test-only');
    assert.equal(sent.body.messages[0].role, 'system');
    assert.equal(sent.body.messages[1].content, 'Namaste');
  } finally {
    globalThis.fetch = previousFetch;
    if (previousKey === undefined) delete process.env.QORVYNT_AGENT_API_KEY;
    else process.env.QORVYNT_AGENT_API_KEY = previousKey;
  }
});
