const assert = require("node:assert/strict");
const path = require("node:path");
const test = require("node:test");

const {
  EclipseClientError,
  EclipseProcessError,
  EclipseScenarioClient,
  assertAtomicAmount,
  assertIdentifier,
} = require("../../sdk");

const ROOT = path.resolve(__dirname, "../..");
const CARGO = process.env.ECLIPSEDTL_CARGO || "cargo";

function client(options = {}) {
  return new EclipseScenarioClient({ cwd: ROOT, cargo: CARGO, ...options });
}

test("client executes a versioned scenario without a shell", () => {
  const report = client().runFile("tests/fixtures/normal_batch.json");

  assert.equal(report.name, "normal_batch");
  assert.equal(report.receipts.length, 1);
  assert.equal(report.receipts[0].operator, "op-alpha");
});

test("client executes an in-memory scenario through a private temporary file", () => {
  const scenario = client().readScenario("tests/fixtures/normal_batch.json");
  scenario.name = "sdk-memory-scenario";
  const report = client().runScenario(scenario);

  assert.equal(report.name, "sdk-memory-scenario");
  assert.equal(report.batches[0].status, "settled");
});

test("client rejects non-json scenario paths before process creation", () => {
  assert.throws(
    () => client().runFile("Cargo.toml"),
    (error) => error instanceof EclipseClientError && /\.json/.test(error.message),
  );
});

test("client returns a typed process error for rejected scenarios", () => {
  const invalid = {
    name: "sdk-invalid-scenario",
    actions: [{ type: "deposit", account: "missing", asset: "EUSD", amount: 1 }],
  };

  assert.throws(
    () => client().runScenario(invalid),
    (error) =>
      error instanceof EclipseProcessError &&
      error.details.status !== 0 &&
      /rejected/.test(error.message),
  );
});

test("client validates identifiers and atomic amounts", () => {
  assert.equal(assertIdentifier("route:eu-west.1"), "route:eu-west.1");
  assert.equal(assertAtomicAmount(10_000), 10_000);
  assert.throws(() => assertIdentifier("route with spaces"), EclipseClientError);
  assert.throws(() => assertAtomicAmount(Number.MAX_SAFE_INTEGER + 1), EclipseClientError);
  assert.throws(() => assertAtomicAmount(-1), EclipseClientError);
});

test("client rejects invalid execution limits", () => {
  assert.throws(() => client({ timeoutMs: 0 }), EclipseClientError);
  assert.throws(() => client({ maxBuffer: -1 }), EclipseClientError);
});
