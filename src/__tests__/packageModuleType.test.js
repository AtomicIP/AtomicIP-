/**
 * Regression test for issue #1320: package.json declared "type": "module"
 * while every file under src/ uses CommonJS. Loading those files through
 * Node's native loader silently produced empty module namespaces.
 */
const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");

const ROOT = path.resolve(__dirname, "../..");

describe("package.json module type (issue #1320)", () => {
  test("does not declare type: module while src/ is CommonJS", () => {
    const pkg = JSON.parse(fs.readFileSync(path.join(ROOT, "package.json"), "utf8"));
    expect(pkg.type).not.toBe("module");
  });

  test("src modules load through native Node require with real exports", () => {
    const script = [
      "const m = require('./src/refunds/refundTracker.js');",
      "process.stdout.write(JSON.stringify(Object.keys(m).sort()));",
    ].join("\n");
    const out = execFileSync(process.execPath, ["-e", script], { cwd: ROOT }).toString();
    const keys = JSON.parse(out);
    expect(keys.length).toBeGreaterThan(0);
    expect(keys).toEqual(expect.arrayContaining(["createRefundRecord", "summarizeRefunds"]));
  });
});
