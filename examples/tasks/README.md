# 本地持久队列报表

只需要 Bun 和本地 PostgreSQL。多个短命 producer 把任务写入持久队列，独立 worker 调用普通 async 报表服务。没有 Notes、Web、邮件、付费接口或存储插件。

队列由 `@lenso/tasks/postgres` 管理；业务证明是示例自己拥有的 Drizzle 表 `task_example_reports`，不是日志或队列中的 result。`reportId` 是稳定业务幂等键，主键 upsert 写入 `sum/count`。相同 `reportId` 重跑只保留一条记录；如果用不同 rows 重新提交同一键，最后一次写入覆盖之前的汇总，不是输入冲突检测。

## 准备

先建立本地数据库。`DATABASE_URL` 必须由运行环境提供，没有默认凭据；不要把真实凭据提交到仓库。所有终端使用相同 URL 和队列名。

从仓库根目录安装并构建框架包：

```sh
bun install
bun run --cwd packages/lenso build
bun run --cwd packages/tasks build
cd examples/tasks
export DATABASE_URL='postgres://localhost/lenso_tasks'
# 可选，默认 reports。producer、worker、migrate 必须一致。
export TASK_QUEUE_NAME=reports
bun src/migrate.ts
```

迁移显式创建 provider 表和业务表；producer、worker 和模块 import 都不会迁移。示例的 `CREATE TABLE IF NOT EXISTS` 只负责首次建表，不是未来 schema 变更的迁移系统。

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

`enqueue` 从 stdin 读取 JSON，stdout 只输出 jobId。`get` 只输出安全状态字段或 `null`，不输出 payload、数据库错误或任务错误文本。`report` 查询业务表，仅输出 `sum/count` 或 `null`。错误输出为固定消息，不打印 URL、凭据、payload、原始 error 或堆栈。worker 的 stderr 开始日志只有框架 jobId 和 attempt，供确认任务已经运行。

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

单元测试覆盖输入默认值、汇总以及 AbortSignal 等待。持久性、claim/recovery、跨进程 cancel/retry 和业务 upsert 需要以上本地 PostgreSQL 命令实测；单元测试不是这些行为的证明。
