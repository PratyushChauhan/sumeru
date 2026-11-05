# Rotate the endpoint token

1. Open **Configure**

![Configure tab showing the bearer token row with Rotate](/images/configure.png)

2. In the bearer token row, click **Rotate**
3. Re-add Sumeru to each agent from **Connect an agent** (see [Connect your agent](./connect-agent.md)), or update any other MCP host config that still has the old `Authorization: Bearer …` value

Old tokens stop working immediately. The new token is stored in the OS keychain (`sumeru` / `endpoint-token`).
