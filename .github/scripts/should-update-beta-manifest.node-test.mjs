import assert from "node:assert/strict";
import test from "node:test";

import {
  compareSemver,
  releaseChannelForVersion,
} from "./should-update-beta-manifest.mjs";

test("orders beta sequence numbers", () => {
  assert.equal(compareSemver("2.0.5-beta.2", "2.0.5-beta.1"), 1);
});

test("orders stable promotion after its prerelease", () => {
  assert.equal(compareSemver("v2.0.5", "v2.0.5-beta.9"), 1);
});

test("does not replace a newer beta with an older stable hotfix", () => {
  assert.equal(compareSemver("2.0.4", "2.0.5-beta.1"), -1);
});

test("accepts equal versions for idempotent publication", () => {
  assert.equal(compareSemver("2.0.5-beta.1", "v2.0.5-beta.1"), 0);
});

test("accepts only stable and numbered beta release versions", () => {
  assert.equal(releaseChannelForVersion("2.0.5"), "stable");
  assert.equal(releaseChannelForVersion("v2.0.5-beta.12"), "beta");
  assert.throws(
    () => releaseChannelForVersion("2.0.5-beta.0"),
    /Unsupported release version/,
  );
  assert.throws(
    () => releaseChannelForVersion("2.0.5-rc.1"),
    /Unsupported release version/,
  );
});

test("rejects invalid versions", () => {
  assert.throws(() => compareSemver("beta", "2.0.5"), /Invalid semantic version/);
});
