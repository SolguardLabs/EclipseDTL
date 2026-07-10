const assert = require("node:assert/strict");
const { spawnSync } = require("node:child_process");
const path = require("node:path");
const test = require("node:test");

const ROOT = path.resolve(__dirname, "../..");
const CARGO = process.env.ECLIPSEDTL_CARGO || "cargo";

function runScenario(name) {
  const scenario = path.join(ROOT, "tests", "fixtures", name);
  const result = spawnSync(CARGO, ["run", "--quiet", "--", "--scenario", scenario], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
}

function findAccount(report, id) {
  const account = report.accounts.find((entry) => entry.id === id);
  assert.ok(account, `missing account ${id}`);
  return account;
}

function balance(report, accountId, assetId) {
  const account = findAccount(report, accountId);
  const cell = account.balances.find((entry) => entry.asset === assetId);
  return cell ? cell.available : 0;
}

function findOperator(report, id) {
  const operator = report.operators.find((entry) => entry.id === id);
  assert.ok(operator, `missing operator ${id}`);
  return operator;
}

function findBatch(report, id) {
  const batch = report.batches.find((entry) => entry.id === id);
  assert.ok(batch, `missing batch ${id}`);
  return batch;
}

function findBid(report, id) {
  const bid = report.bids.find((entry) => entry.id === id);
  assert.ok(bid, `missing bid ${id}`);
  return bid;
}

test("selects the best admitted operator bid and settles the batch", () => {
  const report = runScenario("normal_batch.json");
  const batch = findBatch(report, "batch-001");
  const receipt = report.receipts[0];

  assert.equal(batch.status, "settled");
  assert.equal(batch.selected_bid, "bid-alpha");
  assert.equal(receipt.operator, "op-alpha");
  assert.equal(receipt.fallback, false);
  assert.equal(receipt.gross_out, 10100);
  assert.equal(receipt.operator_fee, 20);
  assert.equal(receipt.net_out, 10080);

  assert.equal(balance(report, "alice", "EUSD"), 10000);
  assert.equal(balance(report, "vault", "EUSD"), 10000);
  assert.equal(balance(report, "vault", "ELIQ"), 39900);
  assert.equal(balance(report, "bob", "ELIQ"), 10080);
  assert.equal(balance(report, "fee-op-alpha", "ELIQ"), 20);
});

test("records guarantee attachment and route exposure for the winning desk", () => {
  const report = runScenario("normal_batch.json");
  const alpha = findOperator(report, "op-alpha");
  const beta = findOperator(report, "op-beta");
  const receipt = report.receipts[0];

  assert.equal(receipt.required_guarantee, 1515);
  assert.equal(receipt.attached_guarantee, 1515);
  assert.equal(alpha.pledged, 5000);
  assert.equal(alpha.locked, 1515);
  assert.equal(alpha.available, 3485);
  assert.equal(alpha.route_exposure[0].route, "route-main");
  assert.equal(alpha.route_exposure[0].committed, 1515);
  assert.equal(beta.locked, 0);

  assert.equal(findBid(report, "bid-alpha").status, "settled");
  assert.equal(findBid(report, "bid-beta").status, "admitted");
});

test("falls back to the next operator when final vault liquidity cannot clear the selected bid", () => {
  const report = runScenario("fallback_batch.json");
  const batch = findBatch(report, "batch-002");
  const receipt = report.receipts[0];
  const fallbackEvent = report.events.find((event) => event.type === "settlement_fallback");

  assert.equal(batch.status, "settled");
  assert.equal(batch.selected_bid, "bid-alpha");
  assert.equal(batch.fallback_bid, "bid-beta");
  assert.equal(receipt.operator, "op-beta");
  assert.equal(receipt.fallback, true);
  assert.equal(receipt.gross_out, 9800);
  assert.equal(receipt.operator_fee, 9);
  assert.equal(receipt.net_out, 9791);

  assert.ok(fallbackEvent);
  assert.equal(fallbackEvent.from_bid, "bid-alpha");
  assert.equal(fallbackEvent.to_bid, "bid-beta");
  assert.equal(findBid(report, "bid-alpha").status, "superseded");
  assert.equal(findBid(report, "bid-beta").status, "settled");
  assert.equal(balance(report, "vault", "ELIQ"), 0);
  assert.equal(balance(report, "treasury", "ELIQ"), 700);
  assert.equal(balance(report, "bob", "ELIQ"), 9791);
  assert.equal(balance(report, "fee-op-beta", "ELIQ"), 9);
});
