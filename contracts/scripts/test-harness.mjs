// Minimal test runner: registers cases, runs them sequentially, reports
// pass/fail with the assertion that broke. Deliberately dependency-free so the
// suite runs anywhere the compiler runs.

const cases = [];
let currentSuite = '';

export function describe(name, body) {
  const previous = currentSuite;
  currentSuite = previous ? `${previous} > ${name}` : name;
  body();
  currentSuite = previous;
}

export function it(name, body) {
  cases.push({ name: currentSuite ? `${currentSuite} > ${name}` : name, body });
}

export function assert(condition, message) {
  if (!condition) throw new Error(message ?? 'assertion failed');
}

export function assertEqual(actual, expected, message) {
  const a = normalize(actual);
  const e = normalize(expected);
  if (a !== e) {
    throw new Error(
      message ?? `expected ${e} but got ${a}`,
    );
  }
}

export function assertBigIntEqual(actual, expected, message) {
  if (BigInt(actual) !== BigInt(expected)) {
    throw new Error(message ?? `expected ${expected} but got ${actual}`);
  }
}

export function assertAddressEqual(actual, expected, message) {
  if (String(actual).toLowerCase() !== String(expected).toLowerCase()) {
    throw new Error(message ?? `expected address ${expected} but got ${actual}`);
  }
}

/** Asserts the promise rejects and, when given, that the message matches. */
export async function assertReverts(promise, match, message) {
  try {
    await promise;
  } catch (error) {
    if (match && !error.message.includes(match)) {
      throw new Error(
        message ?? `expected revert matching "${match}" but got: ${error.message}`,
      );
    }
    return error.message;
  }
  throw new Error(message ?? `expected a revert but the call succeeded`);
}

function normalize(value) {
  if (typeof value === 'bigint') return value.toString();
  if (Array.isArray(value)) return value.map(normalize).join(',');
  return String(value);
}

export async function run() {
  let passed = 0;
  const failures = [];
  const started = Date.now();

  for (const testCase of cases) {
    const caseStart = Date.now();
    try {
      await testCase.body();
      passed += 1;
      process.stdout.write(`  ok   ${testCase.name} (${Date.now() - caseStart}ms)\n`);
    } catch (error) {
      failures.push({ name: testCase.name, error });
      process.stdout.write(`  FAIL ${testCase.name}\n`);
    }
  }

  process.stdout.write(`\n${passed}/${cases.length} passed in ${Date.now() - started}ms\n`);

  for (const failure of failures) {
    process.stdout.write(`\nFAILED: ${failure.name}\n  ${failure.error.message}\n`);
    if (process.env.VERBOSE) process.stdout.write(`${failure.error.stack}\n`);
  }

  return failures.length === 0;
}
