FROM node:20-bookworm

# Install system dependencies
RUN apt-get update && apt-get install -y \
    curl \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install Playwright and Chromium
RUN npx playwright install --with-deps chromium

# Install Vercel CLI globally
RUN npm install -g vercel@latest

# Install Claude Code globally
RUN npm install -g @anthropic-ai/claude-code@latest

# Set up working directory
WORKDIR /app

# Default command: keep the container running
CMD ["sleep", "infinity"]
