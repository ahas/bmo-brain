// The client calls a route of the contract with the token, reads its answer as the contract's type, and
// reads a refusal in the error shape.

import { expect, test } from "bun:test";

import { brainClient, type Memory } from "../index";

const memory: Memory = {
  id: 7,
  scope: "room:a",
  subjectKey: null,
  kind: "fact",
  text: "the service answers health",
  state: "standing",
  conditions: [],
  labels: {},
  dueAt: null,
  matchCount: 0,
  lastMatchedAt: null,
  createdAt: 1_700_000_000_000,
  changedAt: 1_700_000_000_000,
  watches: [],
};

test("a call reaches its route with the token and reads the answer", async () => {
  const asked: Request[] = [];
  const answer = Object.assign(
    async (input: string | URL | Request, init?: RequestInit): Promise<Response> => {
      asked.push(input instanceof Request ? input : new Request(input, init));
      return Response.json(memory, { status: 201 });
    },
    { preconnect: () => {} },
  );
  const client = brainClient("http://brain.test", "the-token", answer);

  const { data, error } = await client.POST("/memories", {
    body: { scope: "room:a", kind: "fact", text: memory.text, conditions: [], labels: {} },
  });

  expect(error).toBeUndefined();
  expect(data?.id).toBe(7);
  expect(asked[0].url).toBe("http://brain.test/memories");
  expect(asked[0].method).toBe("POST");
  expect(asked[0].headers.get("Authorization")).toBe("Bearer the-token");
  expect(asked[0].headers.get("Accept")).toBe("application/json");
});

test("a refusal is read in the error shape", async () => {
  const refuse = Object.assign(
    async (): Promise<Response> =>
      Response.json({ code: "not-found", error: "no memory row stands at 404" }, { status: 404 }),
    { preconnect: () => {} },
  );
  const client = brainClient("http://brain.test", "the-token", refuse);

  const { data, error } = await client.GET("/memories/{memoryId}", {
    params: { path: { memoryId: 404 } },
  });

  expect(data).toBeUndefined();
  expect(error?.code).toBe("not-found");
});
