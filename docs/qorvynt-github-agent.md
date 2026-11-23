# QORVYNT GitHub Agent

The QORVYNT Agent answers public questions inside GitHub Issues. Click the **Talk to QORVYNT Agent** logo in the repository README, create a question, and include **@qorvynt** in the issue body or a follow-up comment.

## Provider setup required

The workflow is provider-neutral and uses an OpenAI-compatible chat-completions API. The repository owner must configure:

1. Repository **Settings → Secrets and variables → Actions → New repository secret**:
   - **QORVYNT_AGENT_API_KEY**: API key for the chosen model provider.
2. Optional repository **Actions variables**:
   - **QORVYNT_AGENT_BASE_URL**: API base URL. Default: https://api.openai.com/v1.
   - **QORVYNT_AGENT_MODEL**: model identifier. Default: gpt-4.1-mini.

Never put provider keys in source files, issue bodies, comments, or browser code. The default OpenAI endpoint may incur provider charges; check the provider's current pricing and limits before enabling it. A compatible provider with an available free tier can be configured by changing the base URL and model.

## How it works

- GitHub Actions listens for new Issues and comments containing **@qorvynt**.
- The workflow reads the issue and up to 12 recent comments.
- It sends the conversation to the configured model provider and posts the answer as a GitHub Actions bot comment.
- The workflow has a three-minute timeout and does not run for pull-request review comments.

## Public conversation and safety

Issues and agent replies are public. Do not include personal data, passwords, API keys, private keys, unreleased secrets, or confidential code. User-provided issue text is treated as untrusted input. The agent is instructed not to invent repository status or claim to change code.

## Enablement checklist

- [ ] Add **QORVYNT_AGENT_API_KEY** as a repository Actions secret.
- [ ] Set provider base URL and model variables if not using the defaults.
- [ ] Ensure Actions are enabled for the repository.
- [ ] Ensure issue creation and GitHub Actions are enabled for the repository.
- [ ] Open a test issue from the README logo and include **@qorvynt**.
- [ ] Confirm the workflow run succeeds and the bot comment appears.

Until the secret is configured, the workflow posts a setup note rather than an AI answer.


## Website chat (new early-access interface)

The same draft also adds:

- `website/agent.html`: responsive QORVYNT-branded chat page with message history, suggested prompts, keyboard submission and clear early-access status.
- `website/api/chat.js`: a Vercel Node.js serverless endpoint. The provider key stays on the server; browser code never receives it.
- A navigation link from the QORVYNT website to the chat page.

### Hosting requirements

GitHub Pages is static hosting and cannot execute `website/api/chat.js`. To run the chat as an actual AI service, deploy the `website/` directory as the Vercel project root (or another Node.js serverless host) and configure these server-side environment variables:

- `QORVYNT_AGENT_API_KEY` — required provider API key (secret).
- `QORVYNT_AGENT_BASE_URL` — optional OpenAI-compatible API base URL; defaults to `https://api.openai.com/v1`.
- `QORVYNT_AGENT_MODEL` — optional model name; defaults to `gpt-4.1-mini`.
- `QORVYNT_ALLOWED_ORIGIN` — optional exact browser origin allowed to call the API; defaults to `https://vikash7644.github.io`.

The default OpenAI endpoint is not free by default and may incur usage charges. The chat page can render on GitHub Pages, but its API call will not work until a serverless backend is deployed and the frontend is configured to call that backend. Do not treat the interface as a live AI service before a real request succeeds.

### Current limitations

- The endpoint is read-only: it cannot commit code, open PRs, run compiler jobs or change repository settings.
- No persistent account/session memory is implemented; each request carries only the recent messages sent by that browser tab.
- This first backend has request-size and message-count limits, but no durable per-user rate limiter, abuse detection, billing cap or authentication. Do not expose it publicly with a paid key until those protections are added.
- The model receives a curated project identity prompt, not a live index of the entire repository. Repository-grounded retrieval, verified compiler execution, GitHub read-only tools, approval-gated write tools and voice are later stages.


### API validation tests

The serverless endpoint now has Node.js built-in tests in `website/api/chat.test.mjs`. From the `website/` directory, run:

```sh
npm run test:agent
```

The tests cover method handling, CORS preflight, missing provider credentials, rejected origins, malformed/oversized message history, and a mocked successful provider response. They do not make paid API calls.

When deploying the website and API together on Vercel, set `QORVYNT_ALLOWED_ORIGIN` to the exact deployed site origin (for example, the Vercel production URL or your verified custom domain). The default value is the existing GitHub Pages origin and will reject browser requests from a different Vercel domain.
