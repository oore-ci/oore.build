-- The MCP server at /mcp answers AI agents only after an owner turns it on.
ALTER TABLE instance_preferences ADD COLUMN mcp_enabled INTEGER NOT NULL DEFAULT 0;
