#!/bin/bash
# Quality gate check script - runs all checks before commit/release
set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${YELLOW}Running quality gate checks...${NC}"

echo -e "${YELLOW}[1/4] Type checking...${NC}"
pnpm typecheck

echo -e "${YELLOW}[2/4] Linting...${NC}"
pnpm lint

echo -e "${YELLOW}[3/4] Frontend tests...${NC}"
pnpm test run

echo -e "${YELLOW}[4/4] Backend tests...${NC}"
pnpm test:backend

echo -e "${GREEN}✓ All quality checks passed!${NC}"
