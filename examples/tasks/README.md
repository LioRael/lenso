# 本地持久队列报表

需要 Bun、本地 PostgreSQL 和应用配置的真实认证源。多个短命 producer 把任务写入持久队列，独立 worker 调用普通 async 报表服务。没有 Notes、Web、邮件、付费接口或存储插件。

队列由 `@lenso/tasks/postgres` 管理；业务证明是示例自己拥有的 Drizzle 表 `task_example_reports`，不是日志或队列中的 result。`reportId` 是稳定业务幂等键，主键 upsert 写入 `sum/count`。相同 `reportId` 重跑只保留一条记录；如果用不同 rows 重新提交同一键，最后一次写入覆盖之前的汇总，不是输入冲突检测。

## 准备

先建立本地数据库。`DATABASE_URL` 必须由运行环境提供，没有默认凭据；不要把真实凭据提交到仓库。所有终端使用相同 URL 和队列名。

从仓库根目录安装并构建框架包：

```sh
bun install
bun run build
cd examples/tasks
export DATABASE_URL='postgres://localhost/lenso_tasks'
# 可选，默认 reports。producer、worker、migrate 必须一致。
export TASK_QUEUE_NAME=reports
bun src/migrate.ts
```

迁移显式创建 provider 表和业务表；producer、worker 和模块 import 都不会迁移。示例的 `CREATE TABLE IF NOT EXISTS` 只负责首次建表，不是未来 schema 变更的迁移系统。

## 认证与持久所有权

`lenso.config.ts` 明确公开 `tasks.submit/query/cancel/retry/report`，方法各接收一个普通业务输入。认证证据不在输入 JSON 中，不能传 `actor`、`subjectId` 或凭据来冒充所有者。`producer.ts` 是同一 `lenso-cli call` 边界的兼容入口，不再直接访问队列。

入口环境必须提供 `TASK_SESSION`（现有短期会话凭据）及 `TASK_AUTH_SOURCE_MODULE`（可信应用模块的绝对路径；相对路径以当前工作目录为基准）。该模块导出 `connectTaskAuth(): Promise<TaskAuthConnection>`，返回 `{source, close?}`。`source` 是真实的 `@lenso/auth` `AuthSource<string | null>`，验证传入凭据并返回实际主体；可复用应用已有的 `createManagedSessions(...).source`，或把实际会话服务通过 `sessionSource` 适配成源。其 realm 如有声明必须为 `task-example`。不要使用环境中的开发者主体、固定测试主体、任意 token 直译为主体的源，或在此入口签发新凭据。测试中的固定证据映射仅是测试夹具，不是生产配置。

资源获取放在 `connectTaskAuth`，不能放在模块顶层。若连接归此入口所有，返回 `close`；借用的共享连接不要关闭。模块不会在 `inspect` 中加载。缺少配置时调用失败，不降级成匿名或管理员；过期、撤销及无法解析证据由真实源拒绝。源码中的 `createAuth(realm(...)).for(audience(...))` 先认证，再在每个业务边界 `enforce` 重新验证证据。

请通过环境或秘密管理器注入凭据，不把真实凭据写进命令、源文件或报告。所有 producer 终端使用同一身份提供者及 realm；worker 和迁移不需要用户凭据。配置好真实源后可查看共享 schema：

```sh
# 从仓库根目录运行；inspect 不要求 DB 或认证连接。
bun packages/cli/src/bin.ts inspect tasks submit --root examples/tasks --json
printf '%s\n' '{"reportId":"authorized-daily","rows":[1,2]}' |
  bun packages/cli/src/bin.ts call tasks submit --root examples/tasks --stdin --json
# 将返回的 jobId 放入普通输入，不传 actor：
printf '%s\n' '{"jobId":"<returned-job-id>"}' |
  bun packages/cli/src/bin.ts call tasks query --root examples/tasks --stdin --json
```

MCP host 可启动 `bun /absolute/path/to/examples/tasks/src/mcp.ts`，沿用同一可信环境。该可选 stdio 入口固定应用 root，allowlist 只包含上述五个授权操作，不启动 worker，不暴露 shell 或动态模块选择。`TASK_AUTH_SOURCE_MODULE` 仅由启动环境配置，客户端不能传入。协议目录复用 operation 描述；MCP 请求取消仍会等待当前调用和 cleanup，不等于 `tasks.cancel`，也不等于后台任务停止。详见 [`@lenso/mcp`](../../packages/mcp/README.md)。

显式迁移新增 `task_example_report_owners` 和 `task_example_job_reports`。`reportId` 的所有权是全库唯一、不可重分配的 `(realmId, subjectId)`，与全库业务表主键一致；同一所有者可按原规则重写报表。队列名 + jobId 通过应用表查到报告及所有者，不能靠知道 jobId 获得访问权。另一所有者对查询、取消、retry、报告读取及相同 reportId 提交均被拒绝，且不会执行队列操作。队列 dedup key 包含所有者和 reportId 的哈希作用域，不能用同名 key 得到别人的任务。

旧报告和旧任务不会自动归给第一次调用者；没有持久所有权记录的资源一律拒绝。需要管理员在验证真实历史所有权后显式迁移，不能由 setup 自动认领。新提交先保留报告所有权，再入队，再记录 job 映射。这不是跨数据库原子事务：入队后的映射写入失败或进程崩溃可能留下不可访问的孤立任务。操作会失败，而不会绕过所有权；使用相同 dedup key 由原所有者重试可补齐映射。不带 dedup key 的重新提交可能产生新任务。数据库操作权限属于部署信任边界，原始 DB/队列客户端不是用户接口。

升级现有数据库时，先停止旧的未授权 producer，并排空或隔离旧队列，再开启新入口。旧 worker payload 没有所有权信息，不能让未知所有权的旧任务与新提交使用同一报表键并发写入；迁移命令不会暂停 worker、扫描旧队列或替管理员决定历史所有权。

## 两个 producer 和一个 worker

终端 A，先入队两个任务，每条命令创建并关闭自己的连接，然后退出：

```sh
A=$(printf '%s\n' '{"reportId":"daily-a","rows":[10,20,-5]}' | bun src/producer.ts enqueue)
B=$(printf '%s\n' '{"reportId":"daily-b","rows":[2,4,6]}' | bun src/producer.ts enqueue)
bun src/producer.ts get "$A"
bun src/producer.ts get "$B"
```

终端 B，独立运行 worker：

```sh
bun src/worker.ts
```

终端 A，重复查询直到 `state` 为 `succeeded`，再读取真正的业务表：

```sh
bun src/producer.ts get "$A"
bun src/producer.ts get "$B"
bun src/producer.ts report daily-a # {"sum":25,"count":3}
bun src/producer.ts report daily-b # {"sum":12,"count":3}
```

`enqueue` 从 stdin 读取 JSON，stdout 只输出 jobId。`get` 只输出安全状态字段或 `null`，不输出 payload、数据库错误或任务错误文本。没有所有权记录或其他所有者的 jobId 返回拒绝，不泄露任务是否存在。`report` 先检查所有权再查询业务表，仅输出 `sum/count` 或 `null`。错误输出为固定消息，不打印 URL、凭据、payload、原始 error 或堆栈。worker 的 stderr 开始日志只有框架 jobId 和 attempt，供确认任务已经运行。

### 输入与延迟

输入为 `reportId`（必填非空字符串）、`rows`（简单有限数字数组）、`failUntilAttempt`（默认 0）、`durationMs`（默认 0）。attempt 从 1 开始，`attempt <= failUntilAttempt` 故意失败；失败发生在业务写入之前。`durationMs` 是合作式等待，用于观察运行中取消。

producer 还接受调度字段 `runAt`（带时区的 ISO 时间）和可选 `deduplicationKey`，它们不属于业务 payload：

```sh
RUN_AT=$(bun -e 'console.log(new Date(Date.now() + 30000).toISOString())')
D=$(printf '{"reportId":"delayed","rows":[1,2,3],"runAt":"%s"}\n' "$RUN_AT" | bun src/producer.ts enqueue)
bun src/producer.ts get "$D"
# runAt 之后由 worker 领取，再查询：
bun src/producer.ts report delayed
```

`deduplicationKey` 是队列侧的入队去重，不替代 `reportId` 的业务写入幂等性：

```sh
printf '%s\n' '{"reportId":"deduplicated","rows":[3,7],"deduplicationKey":"deduplicated-v1"}' | bun src/producer.ts enqueue
printf '%s\n' '{"reportId":"deduplicated","rows":[3,7],"deduplicationKey":"deduplicated-v1"}' | bun src/producer.ts enqueue
```

### 有限失败与手动 retry

任务最多自动尝试 3 次，重试延迟从 2 秒开始退避，上限 10 秒。不要把这些延迟当作精确完成时间。

```sh
R=$(printf '%s\n' '{"reportId":"retry-demo","rows":[8,9],"failUntilAttempt":3}' | bun src/producer.ts enqueue)
bun src/producer.ts get "$R"
# 等待 get 显示 failed、attempt:3，再执行：
bun src/producer.ts retry "$R" # true
# 手动 retry 沿用 jobId/payload，不重置 attempt，增加一次预算。
# 第 4 次执行不再故意失败，等待 succeeded 后：
bun src/producer.ts get "$R"
bun src/producer.ts report retry-demo # {"sum":17,"count":2}
```

`retry` 只接受最终失败任务，其余情况返回 false。故意失败条件和 rows 不会被修改；如果条件仍然成立，新尝试还会失败。也可用 `failUntilAttempt:2` 观察第三次自动尝试成功，无需手动 retry。

### 运行中 cancel 不是 rollback

```sh
C=$(printf '%s\n' '{"reportId":"cancel-demo","rows":[4,5],"durationMs":30000}' | bun src/producer.ts enqueue)
bun src/producer.ts get "$C"
# 确认 running（或 worker 已输出该 jobId 的 report-started）后：
bun src/producer.ts cancel "$C"
bun src/producer.ts get "$C"
bun src/producer.ts report cancel-demo
```

返回值可能是 `requested`、`cancelled`、`terminal` 或 `missing`。运行中 `requested` 只是取消请求，不代表 handler 已停止。这个示例在等待阶段响应 AbortSignal，写入前再次检查 signal，因此及时取消等待中的任务不会新写报表。数据库写入一旦开始就没有 signal 中断或事务回滚承诺；与写入竞争的取消可能留下报告记录。已提交的写入、之前尝试的写入和其他外部副作用都不会自动撤销。

## worker crash / restart

先停止上面的前台 worker（Ctrl-C，等待 drained 日志），只对这里启动的 PID 做 crash：

```sh
CRASH=$(printf '%s\n' '{"reportId":"crash-demo","rows":[11,12],"durationMs":30000}' | bun src/producer.ts enqueue)
bun src/worker.ts &
WORKER_PID=$!
bun src/producer.ts get "$CRASH"
# 重复 get，确认 running 后，仅杀死刚刚启动的 worker：
kill -KILL "$WORKER_PID"
wait "$WORKER_PID" || true
bun src/worker.ts
```

另一个终端保留 `$CRASH`（或复制它的 jobId），查询状态和 `report crash-demo`。crash 不执行清理；任务由 provider 的持久恢复机制在租约/超时条件满足后重新处理，不承诺立即恢复。数据库和队列不会因 producer/worker 进程退出而消失。如果 crash 发生在业务提交之后、队列确认之前，可能再次执行，`reportId` upsert 防止重复业务行，但这不是 exactly-once。

## SIGINT / SIGTERM 策略与资源所有权

第一次 SIGINT/SIGTERM 请求 `worker.stop({abort:true})`：停止领取、合作式通知活跃任务，并等待真实 handler Promise 完成。重复信号不会增加 deadline 或强制退出。drain 完成后先 `queue.close()`，再关闭本进程建立的业务 pg pool。没有提前 `process.exit`，也不使用超时跳过 drain。`worker.done` 同样受到监控，消费失败不会留下空转进程；失败也要等实际 handler 全部结束，再尝试清理连接。

shutdown 的 signal 不等于显式 `queue.cancel(jobId)`，不保证持久状态为 cancelled；provider 可记录失败或安排重试供别的 worker 继续。忽略 signal 的 handler 或未完成的 SQL 会继续占用槽位，shutdown 会继续等。信号既不是已经停止的证明，也不是 rollback。

业务服务借用 caller-owned Drizzle 连接，不关闭它；producer、worker、migrate 只关闭各自建立的资源。共享模块只有声明和工厂，导入不建立连接、启动 worker 或注册进程信号。实际资源获取只在显式入口运行。

## 检查

完成统一安装和框架 build 后：

```sh
bun run typecheck
bun test src
```

单元测试覆盖输入默认值、汇总、AbortSignal 等待、真实 Auth 核心与 CLI invoke 生命周期、所有者/跨所有者行为，以及取消返回值和失败 retry 边界。内存队列/所有权夹具不是 PostgreSQL 持久性或真实会话提供者的证明。

`src/postgres.test.ts` 使用真实 PostgreSQL provider、业务 DB 所有权表和 worker；没有 `TASK_TEST_DATABASE_URL` 时明确跳过。测试使用独立测试数据库，显式迁移后运行，不在 setup/test 中迁移：

```sh
# DATABASE_URL 已由环境注入，指向专用测试数据库。
TASK_QUEUE_NAME=authorization-test bun src/migrate.ts
TASK_TEST_DATABASE_URL="$DATABASE_URL" bun test src/postgres.test.ts src/entry.test.ts
```

`src/entry.test.ts` 从真实 CLI 提交、经真实 SDK stdio 入口查询/拒绝跨所有者/取消，再经 CLI 查询持久状态；认证源是该测试创建的临时夹具，不是生产身份提供者。

测试保留 UUID 命名的报告和任务供检查，关闭自己创建的连接和 worker，不删除共享数据。测试断言另一个资源实例能读取同一持久所有权、跨所有者拒绝、待运行取消、运行中请求取消后实际 settled，以及 attempt 3 最终失败后 retry 并由 attempt 4 写入真实报表。真实身份提供者和跨进程 crash/recovery 仍需以上配置后的命令实测，跳过不能当作通过。
