import assert from "node:assert/strict";
import { describe, it } from "node:test";

// 只测动作分类和审批候选键。最终名单匹配在 useToolAuthorization 的内部闭包中，
// 这里不重写该判定，也不把只读分类或键不相交解释为最终批准或拒绝。
const families = [
  {
    name: "config-manage",
    reads: ["list", "get", "logs"],
    writes: ["set", "delete"],
    legacy: [
      "config-list",
      "config-get",
      "config-logs-read",
      "config-set",
      "config-delete",
    ],
  },
  {
    name: "terminal-manage",
    reads: ["list", "read", "wait"],
    writes: ["open", "send", "focus", "resize", "close"],
    legacy: [
      "terminal-list",
      "terminal-read",
      "terminal-wait",
      "terminal-open",
      "terminal-send",
      "terminal-focus",
      "terminal-resize",
      "terminal-close",
    ],
  },
];

// 只返回 native/crates/snow-mcp/src/builtin.rs 中与本测试有关的公开只读动作。
async function readMetadata() {
  return Object.fromEntries(
    families.map(({ name, reads }) => [name, [...reads]]),
  );
}

function freshPolicy(t, listToolReadonlyActions = readMetadata) {
  const previous = Object.getOwnPropertyDescriptor(globalThis, "window");
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: { snow: { listToolReadonlyActions } },
  });
  t.after(() => {
    if (previous) Object.defineProperty(globalThis, "window", previous);
    else Reflect.deleteProperty(globalThis, "window");
  });

  // 同一用例内保留产品的加载缓存，不让它流入下一用例。
  const url = new URL(
    "../../src/renderer/components/mainContent/chatMessages/utils/toolActionPolicy.ts",
    import.meta.url,
  );
  url.searchParams.set("case", t.name);
  return import(url.href);
}

async function loadedPolicy(t) {
  const policy = await freshPolicy(t);
  await policy.ensureToolActionPolicy();
  return policy;
}

function rawCall(name, args) {
  return {
    name,
    arguments: args,
    interactionId: "p0-approval",
    status: "pending",
  };
}

function actionCall(name, action) {
  return rawCall(name, JSON.stringify({ action }));
}

describe("前端审批候选键", { concurrency: false }, () => {
  it("两种工具的只读动作各自共用工具整名", async (t) => {
    const policy = await loadedPolicy(t);
    for (const { name, reads } of families) {
      for (const action of reads) {
        const call = actionCall(name, action);
        assert.equal(
          policy.isReadonlyActionCall(call),
          true,
          `${name}/${action}`,
        );
        assert.deepEqual(
          policy.toolApprovalKeys(call),
          [name],
          `${name}/${action}`,
        );
      }
    }
  });

  for (const { name, writes, legacy } of families) {
    it(`${name}：写动作各有唯一候选键，不含整名或旧工具名`, async (t) => {
      const policy = await loadedPolicy(t);
      const otherKeys = new Set([name, ...legacy]);
      for (const action of writes) {
        const call = actionCall(name, action);
        const keys = policy.toolApprovalKeys(call);
        assert.equal(policy.isReadonlyActionCall(call), false, action);
        assert.deepEqual(keys, [`${name}:${action}`], action);
        assert.equal(
          otherKeys.has(keys[0]),
          false,
          `重复使用了审批键 ${keys[0]}`,
        );
        otherKeys.add(keys[0]);
      }
    });
  }

  it("顶层 action 去除首尾空白，并用规范化后的值分类和生成键", async (t) => {
    const policy = await loadedPolicy(t);
    for (const [name, raw, action, readonly, key] of [
      ["config-manage", " get\t", "get", true, "config-manage"],
      ["terminal-manage", "\n send ", "send", false, "terminal-manage:send"],
    ]) {
      const call = actionCall(name, raw);
      assert.equal(policy.parseToolAction(call.arguments), action);
      assert.equal(policy.isReadonlyActionCall(call), readonly);
      assert.deepEqual(policy.toolApprovalKeys(call), [key]);
    }
  });

  it("未知、大小写不符或仅在另一工具只读的动作保留各自的候选键", async (t) => {
    const policy = await loadedPolicy(t);
    for (const [name, action] of [
      ["config-manage", "future-action"],
      ["config-manage", "GET"],
      ["terminal-manage", "get"],
    ]) {
      const call = actionCall(name, action);
      assert.equal(policy.parseToolAction(call.arguments), action);
      assert.equal(
        policy.isReadonlyActionCall(call),
        false,
        `${name}/${action}`,
      );
      assert.deepEqual(policy.toolApprovalKeys(call), [`${name}:${action}`]);
    }
  });

  it("缺失或无效 action：解析为 null，已加载策略不视为只读，但候选键仍是整名", async (t) => {
    const policy = await loadedPolicy(t);
    assert.equal(policy.parseToolAction(undefined), null);
    for (const args of [
      "",
      "{",
      "null",
      "[]",
      '"get"',
      "{}",
      '{"nested":{"action":"get"}}',
      '{"action":null}',
      '{"action":1}',
      '{"action":"  "}',
    ]) {
      assert.equal(policy.parseToolAction(args), null, `参数：${args}`);
      for (const { name } of families) {
        const call = rawCall(name, args);
        assert.equal(
          policy.isReadonlyActionCall(call),
          false,
          `${name}/${args}`,
        );
        assert.deepEqual(
          policy.toolApprovalKeys(call),
          [name],
          `${name}/${args}`,
        );
      }
    }
  });

  it("加载失败不缓存失败结果；并发重试共用一次请求，成功后复用缓存", async (t) => {
    let attempts = 0;
    const policy = await freshPolicy(t, async () => {
      attempts += 1;
      if (attempts === 1) throw new Error("测试：只读动作元数据暂不可用");
      return readMetadata();
    });
    const call = actionCall("config-manage", "set");

    await policy.ensureToolActionPolicy();
    assert.equal(attempts, 1);
    assert.equal(policy.hasActionPolicy(call.name), false);
    assert.deepEqual(policy.toolApprovalKeys(call), ["config-manage"]);

    await Promise.all([
      policy.ensureToolActionPolicy(),
      policy.ensureToolActionPolicy(),
    ]);
    assert.equal(attempts, 2);
    assert.equal(policy.hasActionPolicy(call.name), true);
    assert.equal(policy.isReadonlyActionCall(call), false);
    assert.deepEqual(policy.toolApprovalKeys(call), ["config-manage:set"]);

    await policy.ensureToolActionPolicy();
    assert.equal(attempts, 2);
  });
});
