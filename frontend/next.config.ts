import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  output: "standalone",
  poweredByHeader: false,
  // The repository root CLAUDE.md is the only agent entry point.
  agentRules: false,
};

export default nextConfig;
