import { AutoScalingClient, SetDesiredCapacityCommand } from "@aws-sdk/client-auto-scaling";

const autoscaling = new AutoScalingClient({ region: process.env.EMBERLANE_AWS_REGION || process.env.AWS_REGION });

export function isAuthorized(headers = {}, apiKey = process.env.API_KEY || "") {
  if (!apiKey) return true;
  const gotAuth = Object.entries(headers).find(([key]) => key.toLowerCase() === "authorization")?.[1] || "";
  const gotApiKey = Object.entries(headers).find(([key]) => key.toLowerCase() === "x-api-key")?.[1] || "";
  return gotAuth === `Bearer ${apiKey}` || gotApiKey === apiKey;
}

export function warmingResponse() {
  const retryAfter = Number(process.env.RETRY_AFTER_SECS || "5");
  return {
    statusCode: 202,
    headers: { "content-type": "application/json", "retry-after": String(retryAfter) },
    body: JSON.stringify({
      ok: true,
      data: {
        state: "waking",
        message: "Runtime is warming",
        retry_after_secs: retryAfter
      }
    })
  };
}

export async function healthy(fetchImpl = fetch) {
  try {
    const baseUrl = process.env.BASE_URL.replace(/\/$/, "");
    const path = process.env.HEALTH_PATH || "/health";
    const response = await fetchImpl(`${baseUrl}${path}`, { 
      signal: AbortSignal.timeout(2000),
      headers: {
        "X-Emberlane-Secret": process.env.ALB_SECRET || ""
      }
    });
    return response.ok;
  } catch {
    return false;
  }
}

export async function wakeAsg(client = autoscaling) {
  await client.send(new SetDesiredCapacityCommand({
    AutoScalingGroupName: process.env.ASG_NAME,
    DesiredCapacity: Number(process.env.DESIRED_CAPACITY_ON_WAKE || "1"),
    HonorCooldown: false
  }));
}

export async function waitForHealth(fetchImpl = fetch) {
  const fastWaitSecs = Number(process.env.FAST_WAIT_SECS || "25");
  const startupSecs = Number(process.env.STARTUP_TIMEOUT_SECS || "600");
  const deadline = Date.now() + Math.min(fastWaitSecs, startupSecs) * 1000;
  while (Date.now() < deadline) {
    if (await healthy(fetchImpl)) return true;
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
  return false;
}

function method(event) {
  return (event?.requestContext?.http?.method || event?.httpMethod || "GET").toUpperCase();
}

function path(event) {
  return event?.rawPath || event?.path || "/";
}

function body(event) {
  if (!event?.body) return undefined;
  return event.isBase64Encoded ? Buffer.from(event.body, "base64") : event.body;
}

function httpStream(responseStream, statusCode, headers = {}) {
  return awslambda.HttpResponseStream.from(responseStream, {
    statusCode,
    headers
  });
}

function responseStreamJson(responseStream, statusCode, payload, headers = {}) {
  const stream = httpStream(responseStream, statusCode, {
    "content-type": "application/json",
    ...headers
  });
  stream.write(JSON.stringify(payload));
  stream.end();
}

export async function proxyReadyRequest(event, responseStream, fetchImpl = fetch) {
  const baseUrl = process.env.BASE_URL.replace(/\/$/, "");
  const upstream = await fetchImpl(`${baseUrl}${path(event)}`, {
    method: method(event),
    headers: { 
      "content-type": "application/json",
      "X-Emberlane-Secret": process.env.ALB_SECRET || ""
    },
    body: method(event) === "GET" ? undefined : body(event)
  });
  const contentType = upstream.headers.get("content-type") || "application/json";
  const stream = httpStream(responseStream, upstream.status, { "content-type": contentType });
  if (contentType.includes("text/event-stream") && upstream.body) {
    for await (const chunk of upstream.body) {
      stream.write(chunk);
    }
  } else {
    stream.write(Buffer.from(await upstream.arrayBuffer()));
  }
  stream.end();
}

export const handler = awslambda.streamifyResponse(async (event, responseStream) => {
  if (!isAuthorized(event.headers || {})) {
    responseStreamJson(responseStream, 401, {
      ok: false,
      error: { code: "auth_required", message: "authorization required", details: {} }
    });
    return;
  }
  if (!(await healthy())) {
    await wakeAsg();
    if ((process.env.MODE || "fast").toLowerCase() === "slow" || !(await waitForHealth())) {
      const warming = warmingResponse();
      responseStreamJson(responseStream, 202, JSON.parse(warming.body), {
        "retry-after": warming.headers["retry-after"]
      });
      return;
    }
  }
  await proxyReadyRequest(event, responseStream);
});
