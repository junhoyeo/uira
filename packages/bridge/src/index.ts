import * as readline from 'readline';
import { query } from '@anthropic-ai/claude-agent-sdk';

interface BridgeRequest {
  id: string;
  method: 'query' | 'ping';
  params?: QueryParams;
}

interface QueryParams {
  prompt: string;
  options?: {
    systemPrompt?: string;
    agents?: Record<string, AgentDef>;
    mcpServers?: Record<string, McpServerDef>;
    allowedTools?: string[];
    permissionMode?: string;
  };
}

interface AgentDef {
  description: string;
  prompt: string;
  tools?: string[];
  model?: string;
}

interface McpServerDef {
  command: string;
  args: string[];
  env?: Record<string, string>;
}

interface BridgeResponse {
  id: string;
  result?: unknown;
  error?: { code: number; message: string };
}

interface StreamMessage {
  id: string;
  stream: true;
  data: unknown;
}

function sendResponse(response: BridgeResponse | StreamMessage): void {
  console.log(JSON.stringify(response));
}

function sendError(id: string, code: number, message: string): void {
  sendResponse({ id, error: { code, message } });
}

async function handleQuery(id: string, params: QueryParams): Promise<void> {
  try {
    const queryOptions = {
      prompt: params.prompt,
      options: params.options || {},
    };

    for await (const message of query(queryOptions as Parameters<typeof query>[0])) {
      sendResponse({
        id,
        stream: true,
        data: message,
      });
    }

    sendResponse({ id, result: { done: true } });
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    sendError(id, -32000, errorMessage);
  }
}

async function handleRequest(request: BridgeRequest): Promise<void> {
  switch (request.method) {
    case 'ping':
      sendResponse({ id: request.id, result: { pong: true, version: '0.1.0' } });
      break;

    case 'query':
      if (!request.params) {
        sendError(request.id, -32602, 'Missing params for query');
        return;
      }
      await handleQuery(request.id, request.params);
      break;

    default:
      sendError(request.id, -32601, `Unknown method: ${request.method}`);
  }
}

async function main(): Promise<void> {
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    terminal: false,
  });

  console.error('[astrape-bridge] Started, waiting for requests...');

  for await (const line of rl) {
    if (!line.trim()) continue;

    try {
      const request = JSON.parse(line) as BridgeRequest;
      await handleRequest(request);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`[astrape-bridge] Parse error: ${errorMessage}`);
      sendError('unknown', -32700, `Parse error: ${errorMessage}`);
    }
  }
}

main().catch((error) => {
  console.error('[astrape-bridge] Fatal error:', error);
  process.exit(1);
});
