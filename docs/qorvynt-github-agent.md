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
