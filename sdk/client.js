const { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } = require("node:fs");
const { tmpdir } = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const DEFAULT_TIMEOUT_MS = 15_000;
const DEFAULT_MAX_BUFFER = 8 * 1024 * 1024;
const MAX_ERROR_OUTPUT = 4_096;
const IDENTIFIER = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/;

class EclipseClientError extends Error {
  constructor(message, details = {}) {
    super(message);
    this.name = "EclipseClientError";
    this.details = Object.freeze({ ...details });
  }
}

class EclipseProcessError extends EclipseClientError {
  constructor(message, details = {}) {
    super(message, details);
    this.name = "EclipseProcessError";
  }
}

function assertIdentifier(value, field = "identifier") {
  if (typeof value !== "string" || !IDENTIFIER.test(value)) {
    throw new EclipseClientError(`${field} has an invalid format`, { field });
  }
  return value;
}

function assertAtomicAmount(value, field = "amount") {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new EclipseClientError(`${field} must be a non-negative safe integer`, { field });
  }
  return value;
}

function assertPositiveInteger(value, field) {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new EclipseClientError(`${field} must be a positive safe integer`, { field });
  }
  return value;
}

function parseReport(stdout) {
  let report;
  try {
    report = JSON.parse(stdout);
  } catch (error) {
    throw new EclipseProcessError("protocol returned invalid JSON", {
      cause: error.message,
      output: stdout.slice(0, MAX_ERROR_OUTPUT),
    });
  }
  for (const field of ["events", "accounts", "operators", "routes", "batches", "bids", "receipts"])
    if (!Array.isArray(report[field])) {
      throw new EclipseProcessError("protocol report is missing a required collection", { field });
    }
  return report;
}

class EclipseScenarioClient {
  constructor(options = {}) {
    this.cwd = path.resolve(options.cwd || path.resolve(__dirname, ".."));
    this.cargo = options.cargo || process.env.ECLIPSEDTL_CARGO || "cargo";
    this.timeoutMs = assertPositiveInteger(options.timeoutMs ?? DEFAULT_TIMEOUT_MS, "timeoutMs");
    this.maxBuffer = assertPositiveInteger(options.maxBuffer ?? DEFAULT_MAX_BUFFER, "maxBuffer");
    this.env = Object.freeze({ ...process.env, ...(options.env || {}) });
  }

  runFile(filePath) {
    if (typeof filePath !== "string" || filePath.length === 0) {
      throw new EclipseClientError("scenario path is required");
    }
    const resolved = path.resolve(this.cwd, filePath);
    if (path.extname(resolved).toLowerCase() !== ".json") {
      throw new EclipseClientError("scenario file must use the .json extension", { path: resolved });
    }
    if (!existsSync(resolved)) {
      throw new EclipseClientError("scenario file does not exist", { path: resolved });
    }

    const result = spawnSync(this.cargo, ["run", "--quiet", "--", "--scenario", resolved], {
      cwd: this.cwd,
      encoding: "utf8",
      env: this.env,
      maxBuffer: this.maxBuffer,
      shell: false,
      timeout: this.timeoutMs,
      windowsHide: true,
    });
    if (result.error) {
      throw new EclipseProcessError("protocol process could not complete", {
        cause: result.error.message,
        path: resolved,
      });
    }
    if (result.status !== 0) {
      throw new EclipseProcessError("protocol rejected the scenario", {
        path: resolved,
        status: result.status,
        stderr: String(result.stderr || "").slice(0, MAX_ERROR_OUTPUT),
      });
    }
    return parseReport(result.stdout);
  }

  runScenario(scenario) {
    if (!scenario || typeof scenario !== "object" || Array.isArray(scenario)) {
      throw new EclipseClientError("scenario must be an object");
    }
    assertIdentifier(scenario.name, "scenario.name");
    if (!Array.isArray(scenario.actions)) {
      throw new EclipseClientError("scenario.actions must be an array");
    }

    const directory = mkdtempSync(path.join(tmpdir(), "eclipsedtl-client-"));
    const file = path.join(directory, "scenario.json");
    try {
      writeFileSync(file, `${JSON.stringify(scenario, null, 2)}\n`, {
        encoding: "utf8",
        flag: "wx",
        mode: 0o600,
      });
      return this.runFile(file);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  }

  readScenario(filePath) {
    const resolved = path.resolve(this.cwd, filePath);
    return JSON.parse(readFileSync(resolved, "utf8"));
  }
}

module.exports = {
  EclipseClientError,
  EclipseProcessError,
  EclipseScenarioClient,
  assertAtomicAmount,
  assertIdentifier,
  parseReport,
};
