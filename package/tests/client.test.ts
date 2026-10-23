// The client calls a route of the contract with the token, reads its answer as the contract's type, and
// reads a refusal in the error shape.

import { expect, test } from "bun:test";

import { brainClient, type Layer } from "../index";

const layer: Layer = {
  id: 7,
  brainId: 3,
  name: "persona",
  description: "who the agent is",
  position: 1,
  createdAt: 1_700_000_000_000,
};

test("a call reaches its route with the token and reads the answer", async () => {
  const asked: Request[] = [];
  const answer = Object.assign(
    async (input: string | URL | Request, init?: RequestInit): Promise<Response> => {
      asked.push(input instanceof Request ? input : new Request(input, init));
      return Response.json(layer);
    },
    { preconnect: () => {} },
  );
  const client = brainClient("http://brain.test", "the-token", answer);

  const { data, error } = await client.PATCH("/brains/{brainId}/layers/{layerId}", {
    params: { path: { brainId: 3, layerId: 7 } },
    body: { position: 1 },
  });

  expect(error).toBeUndefined();
  expect(data?.id).toBe(7);
  expect(asked[0].url).toBe("http://brain.test/brains/3/layers/7");
  expect(asked[0].method).toBe("PATCH");
  expect(asked[0].headers.get("Authorization")).toBe("Bearer the-token");
  expect(asked[0].headers.get("Accept")).toBe("application/json");
});

test("a refusal is read in the error shape", async () => {
  const refuse = Object.assign(
    async (): Promise<Response> =>
      Response.json({ code: "not-found", error: "There is no brain 404." }, { status: 404 }),
    { preconnect: () => {} },
  );
  const client = brainClient("http://brain.test", "the-token", refuse);

  const { data, error } = await client.GET("/brains/{brainId}", {
    params: { path: { brainId: 404 } },
  });

  expect(data).toBeUndefined();
  expect(error?.code).toBe("not-found");
});
