# @lenso/media

Storage Files 之上的图片派生包。普通 async 服务负责请求、查询、取消与取得内部文件引用；已有 `@lenso/tasks` 负责持久化任务、租约、重试和有界执行。不是上传系统、Files 替代品、CDN、视频平台或新的任务系统。

## 入口与能力

| 入口           | Bun                                      | Workers                                |
| -------------- | ---------------------------------------- | -------------------------------------- |
| `@lenso/media` | 服务、preset、状态与适配契约             | 同一控制面，无原生图片处理             |
| `/files`       | 复用现有 Files 授权/读取/上传/删除       | 同一接口，可借用 R2 Files              |
| `/tasks`       | 绑定现有 Tasks 定义与队列                | 生产/查询/取消，原生执行必须委派       |
| `/sqlite`      | 借用 `bun:sqlite`，不关闭调用方数据库    | 不支持                                 |
| `/d1`          | 可用于测试或宿主提供的可信 D1 访问桥     | 原生 D1 binding；不接受 session facade |
| `/bun`         | 子进程中的 sharp 0.35.5 / libvips 8.18.7 | **不得导入或打包**                     |

只有 `/bun` 使用原生依赖。sharp 是精确版本的可选 peer；Bun 执行宿主显式安装它。没有 Rust toolchain 要求。包根、`/files`、`/tasks`、`/d1` 不导入 sharp、文件系统或 Bun SQLite。D1 保存状态，不执行图片解码。Workers 沿用 Tasks 的 `nodejs_compat` 要求，构建保留 `node:*` externals，不使用浏览器 polyfill 替换其 tracing/JSON 依赖。

Workers 控制面省略 `processor`，但配置与实际 Bun executor 相同的 `processorVersion`。`request()` 返回 `pending` 仅表示请求已被队列接受，**不是处理成功或 executor 可用证明**。没有处理器、版本不符或加载失败时，executor 记录 `unavailable`，绝不写 `ready`。本包不提供部署发现、Worker-to-Bun 网络桥或公开管理路由；宿主必须用可信内部适配器委派到既有 Tasks，不能让 D1 task handler 在 Workers 内调用 `/bun`。测试中的 Bun 访问 Miniflare D1 proxy 是本地测试机制，不是生产 D1-to-Bun 通道。

## 最小接入

先通过宿主既有 migration 入口显式应用 `migrations/sqlite/0001_media.sql` 或 `migrations/d1/0001_media.sql`。工厂、导入、配置和 setup 不创建表；数据库、Files、Tasks 都由其原 owner 管理。`createMediaPlugin({id, requires, setup})` 用现有 Core 入口绑定**实际安装的**依赖对象，setup 返回 `MediaOptions`；它不启动 worker。

接入现有 Files 时，**同时**安装 journal queries 和 authorizer，不能只包上传方法：

```ts
import { createMedia } from "@lenso/media";
import { createMediaFileJournal, createFilesMediaStorage } from "@lenso/media/files";
import { createMediaTask, createTasksMediaAdapter } from "@lenso/media/tasks";
import { createBunImageProcessor } from "@lenso/media/bun";
import type { Media, Preset } from "@lenso/media";

const presets: Preset[] = [
  {
    name: "notes-attachment",
    version: "1",
    width: 320,
    height: 320,
    fit: "cover",
    formats: ["webp", "png"],
    defaultFormat: "webp",
    quality: 80,
    metadata: "strip",
    animation: "reject",
  },
];

// 以下放在宿主 resource setup/assembly 中；不要在配置顶层打开数据库。
// Access 来自既有 Auth，不是业务 JSON。
let media: Media<Access>;
const journal = createMediaFileJournal({ store, queries: existingFileQueries });
const fileAuthorize = journal.authorizer({
  authorize: existingFilePolicy, // 仍然检查 tenant/owner/action，含受限 delegate
  authorizeDelivery: (access: Access, id) => media.authorizeDelivery(access, id),
});
// 把 journal.queries 和 fileAuthorize 注入现有 createFilesPlugin。
// 取得该实例的 files；禁止另开一个没有 guard 的 Files 入口访问同一记录。
const task = createMediaTask({ execute: (id, context) => media.execute(id, context) });
// 在原 Tasks 队列中显式注册这个 task，取得该队列 queue。
const processor = createBunImageProcessor();
media = createMedia<Access>({
  store,
  storage: createFilesMediaStorage({ files, journal, storageId: privateStorage.id }),
  tasks: createTasksMediaAdapter(queue, task),
  presets,
  processor,
  processorVersion: processor.version,
  scope: authenticatedScope,
  authorizeDerive: existingDerivationPolicy,
  delegate: resolveLiveMinimalDelegate,
});
// 独立 executor 在服务就绪后启动已有 Tasks worker，例如 concurrency: 2。
// 停止时先停止领取、等待 handlers，再关闭队列与借用资源的 owner。
```

这是接线片段，`Access`、已授权 resource、策略与 queue 由宿主提供，不是第二个应用。可运行闭环位于 `test/notes-fixture.ts` / `test/integration.test.ts`：已有 Notes attachment、真实 Files/local Storage、真实本地 D1 Tasks、Bun 原生 executor。fixture 的 WeakSet actor 只模拟可信认证边界；不是对完整 Notes Auth 登录流程的替代测试。`examples/notes-attachment.ts` 是借用同一服务的最小调用示例，不修改 Notes 应用或 Console。

```ts
import { fileReference } from "@lenso/media/files";

const source = fileReference(await files.metadata(authenticatedAccess, attachmentFileId));
const accepted = await media.request(authenticatedAccess, {
  source,
  preset: "notes-attachment", // 仅可额外选择 preset.formats 内的 format
});
const state = await media.status(authenticatedAccess, accepted.id);
if (state.state === "ready") {
  const result = await media.result(authenticatedAccess, accepted.id);
  const download = await files.read(authenticatedAccess, result.fileId);
  // 流式传给原有授权 Fetch 下载路径；不要把图片字节放入 CLI JSON。
}
const metadataJob = await media.requestMetadata(authenticatedAccess, source);
// 完成后 status.metadata 包含实际编码格式、MIME、尺寸、orientation、alpha 和帧数。
```

输入只接受 `{fileId, version:{revision, etag}}`。必须是 ready、非空 ETag 的原始 Files；派生链首版拒绝。每次检查都走 `files.metadata` 和 `files.read`，并比较实际下载 ETag/尺寸与受控 revision。底层对象 key 不是身份或权限证明；Media 不接收 URL、bucket key、脚本、任意尺寸、任意质量或 EXIF 保留选项。格式由签名与完整解码确定，不相信扩展名和上传声明。

## Preset、身份与状态契约

Preset 有命名 `name`、`version`、固定宽高（1–4096）、`cover|inside|contain`、允许的 `jpeg|png|webp` 集合、默认格式、固定质量（1–100）、`metadata:"strip"`、`animation:"reject"`。调用者只能选已命名 preset 和它允许的格式。`cover` 裁剪；`inside` 保留比例；`contain` 透明填充；本版允许放大小图。

派生 ID 是规范化身份数组的 SHA-256，包括 schema version、tenant、请求主体、授权隔离域、source file ID、revision、ETag、source storage/owner、preset name/version、全部输出 recipe、processor version、持久化队列 identity 和 task name。默认格式与显式相同格式得到同一 ID。不同租户、主体或授权隔离域不会因相同内容哈希复用权限。不是按 filename/可变 fileId 去重。

默认 processor fingerprint 是 `sharp-0.35.5/vips-8.18.7/protocol-1`；原生端实际验证这两个库版本，版本不符返回 `unavailable`。修改处理策略应升级 protocol fingerprint；升级配方或 processor 产生新身份，旧结果不会被覆盖。

| state       | 含义                                                                    |
| ----------- | ----------------------------------------------------------------------- |
| `pending`   | 已接受或等待自动重试；可能保留上一次安全错误                            |
| `running`   | 某次 fenced attempt 执行中                                              |
| `ready`     | Media 条件登记已提交；metadata job 没有派生文件                         |
| `failed`    | 永久业务失败、耗尽尝试或 Tasks 终态协调出的失败                         |
| `cancelled` | durable cancel 已禁止进一步登记；不证明所有旧 provider 请求都已物理停止 |

`stage` 为 `queued → download → decode → transform → upload → register`；metadata-only 跳过 transform/upload。`status.metadata` 是**源图** metadata，输出 metadata/MIME 通过授权 Files 记录和解码取得。`result` 只包含内部 Files 引用，不含 URL、key、凭据。`error` 是固定 `{code,stage,retryable}`；原始异常保留在进程内，不进入状态/Tasks 结果。`cleanupPending` 表示当前 journal 尚有未完成删除的非结果文件，不证明所有旧 writer 已静止。

并发请求先 create-only 插入相同 Media 身份，再用既有 Tasks durable dedup key 取得一个 job。丢失入队响应后可重复同一请求，不新建任务。队列映射、job 与 source/preset 身份不在重试时变化；每次执行使用 Tasks 的递增 attempt fence 和随机 execution token。旧 attempt 可以完成自己的上传，但不能登记或覆盖新结果。每次输出使用 Files 自己的新 identity/key，不复用可覆盖对象。

依赖故障、超时与 native unavailable 可重试，处理器永久拒绝、源版本失效和权限拒绝不可重试。首个 Tasks 定义总共尝试三次，延时 2 秒、指数退避、上限 60 秒。永久业务失败由 Media 保存，handler 正常结束以避免无效重试，所以 Tasks `succeeded` **不等于** Media `ready`。对终态可重试失败，`media.retry` 先调用 Tasks retry，再条件投影 pending；丢失 retry 回应时，更高 attempt 可直接接管可重试失败。Tasks provider 的保留/去重规则仍有效，已剪除 job 不会凭空重建。

`cancel` 对 pending 禁止领取，对 running 提交请求并阻止 Media 登记；Tasks 的 signal 仍是合作式取消。Bun native child 收到 signal/超时会 SIGKILL 并等待退出后释放 slot；Storage/DB 调用仍遵守其提供者自己的取消能力。已经提交的最终写入可能先于取消完成，不能声称回滚 ready。

## 权限与所有权

| 责任                  | Owner / 规则                                                                       |
| --------------------- | ---------------------------------------------------------------------------------- |
| 源文件                | 原 Files owner；保留 tenant/owner，不迁移、不改权限                                |
| 派生文件              | 同一 source owner 与 tenant，写入宿主指定 private Storage                          |
| 请求主体              | 原 Auth 认证主体；有 source metadata/read 和额外申请派生权限                       |
| execute delegate      | 宿主根据持久化主体实时解析；仅该 source 的 metadata/read、该 attempt 的受限 upload |
| cleanup delegate      | 独立维护主体，仅 journal 中该派生的 delete；撤销请求主体后仍能回收                 |
| 文件删除/对象补偿     | Files/Storage 保持唯一物理删除 owner；Media 仅调用其删除状态机                     |
| 保留与 reconciliation | 应用数据 owner 决定期限、调度及 tombstone 收口，本包不自动扫全库                   |

`scope` / `authorizeDerive` / `delegate` 是可信服务配置，不从任务 payload 或 JSON 取权限。Tasks payload 只有 `derivationId`，没有 actor、签名 URL、token 或临时凭据。请求、执行、上传前、登记前以及交付时重新检查源文件权限/版本。`journal.authorizer` 在 raw Files metadata/read/signDownload 路径检查登记与实时 `authorizeDelivery`，所以绕过 Media 直接读派生 fileId 也不能绕过撤权。

必须保留 journal 并让所有访问同一 Files 数据集的入口采用该 authorizer；已有不带 guard 的 Files 实例不能安全地混用。基础 Files policy 仍负责验证上传 assignment 与 delegate scope，`staging(filename)` 只是进程内关联信息，**不是授权凭据**。不能用相同 filename、key 或 hash 认可任意 actor。

如果宿主需要 signed download，沿原 `files.signDownload` 路径按需短时签发；本包不签发/持久化它。URL 是临时 bearer 凭据，不能写日志、文档、任务身份、事件或 analytics；撤权不能撤回已经发出的 URL，短 TTL 和现有 Storage 行为仍适用。

## 清理与恢复

Storage 与数据库不是一个原子事务。journal 在 Files 插入/对象写入前保存 `fileId + derivationId + fence + executionId`；Files 继续管理 pending/uploading/ready/failed/deleting/deleted。Media CAS 发布失败时，未登记文件被 authorizer 隐藏，恢复通过 `Files.delete` 执行，不直接删除 object key。

成功结果不会被 recovery 误删；成功登记响应丢失时重新读取 ready，保留文件。失败/取消/旧 fence 的 artifacts 被标为 discarding，删除失败留在 journal，返回可定位的 `{fileId,state:"failed"}`，不会伪装处理成功。恢复 abandoned uploading 时先通过 Files revision 条件更新使旧 publication 无效，再复用原删除路径。

`deleted` tombstone 只表示**上次删除完成**，不是 writer 静止证明。旧 PUT 可能在该删除之后落盘。`recover(id)` 会再次处理 deleted tombstone，并让 Files `deleted → deleting` 的条件更新重新触达 provider；正常 handler finally 也会重做清理。真实 late-PUT 测试覆盖先删、晚写、再次删的路径。

宿主运维流程：

1. 从已有 Tasks/Media 失败记录或 `media_artifacts` 的 derivation index 分页定位 ID，调用可信内部 `media.recover(id)`，记录返回的安全 ID/状态；不要把它直接暴露为未授权 CLI/MCP。
2. 该入口先按精确 revision 协调 Tasks 终态，再回收，因此源权限已撤销、最后一次 handler 已崩溃时不依赖请求者查询状态。
3. 依赖恢复后重复 failed/discarding 和 deleted tombstone；旧 executor/provider 请求停止并 drain 后再作最后一次 reconciliation。跨进程崩溃/模糊 provider 回应仍可能暂留孤儿对象，不保证所有崩溃点“零孤儿”。
4. journal/Media/Tasks retention 由同一数据 owner 收口。不要先删 journal，再让该 file 被误判为普通文件；published 或残留 Files 尚存在时保留 association。清理队列去重记录遵守 Tasks 原规则。

删除源文件立刻使旧派生交付失败，但**不自动物理级联**。应用数据 owner 根据保留策略用其受限 Files 清理主体删除关联派生；显式删除派生后，查询旧引用失败，不自动新建相同身份的结果。重新生成需要 owner 明确升级配方/处理器身份。临时资源正常返回、超时和取消后由 `/bun` 清理；进程被硬杀可能留下 `lenso-media-*` 临时目录，宿主只能在确认旧进程停止后清理其自有目录，不能任意删除系统 tmp。Bun child 禁用自动安装和 `.env` 加载，并启用 no-orphans。

## 图片与资源限制

仅 JPEG、PNG、WebP。验证签名与 decoded format，inspect 也强制完整像素解码；PNG 另验证完整 chunk/CRC。损坏/截断文件不以 header metadata 冒充成功。SVG/GIF/其他主动或未支持格式拒绝。所有动画 PNG/WebP 拒绝；首版**不实现首帧提取**，`animation:"reject"` 明示此策略。

输出 auto-orient、转 sRGB、剥离 EXIF/GPS/ICC 等 metadata；源 metadata 保留原编码宽高和 orientation 数字，不暴露 EXIF 内容。PNG/WebP 保留 alpha，contain 透明填充；JPEG 明确白底 flatten。质量与尺寸固定在 preset，输出实际 MIME 为编码器格式。

| 限额                                           | 默认                                |
| ---------------------------------------------- | ----------------------------------- |
| Media 下载 / Files 派生输出                    | 16 MiB / 8 MiB                      |
| Processor 编码输入 / 输出                      | 各 20 MiB，实际有效上限取层间较小值 |
| 单维 / 总像素 / 帧数                           | 8192 / 33,554,432 / 1               |
| 子进程 deadline / Media 全阶段 signal deadline | 10 秒 / 30 秒                       |
| 单 child RSS budget / 临时目录 budget          | 512 MiB / 1 MiB                     |
| Native 本实例 concurrency / 等待队列           | 2 / 2                               |

输入、输出、像素、维度、帧数和 admission 都有拒绝边界。libvips cache 关闭，native 内部 concurrency 固定 1；下载、协议、raw decode、输出仍使用有界内存缓冲，不是零拷贝流式图片系统。Tasks worker concurrency 是另一层本实例上限；多进程/多租户总额度仍需宿主 Limits/调度策略。

**RSS 和临时磁盘是观测预算，不是 OS 级硬隔离。** child 每 10 ms 检查 RSS，退出后检查 OS high-water RSS；父进程每 10 ms 检查目录逻辑大小并在退出后检查。采样间内存突增、短命临时文件可能超过预算；stdout 完整缓冲前编码器也可能分配输出。对不可信高风险输入的严格 RSS/disk 保障，部署 owner 必须再加容器/cgroup/volume quota 等宿主约束。Windows/Linux 的 native 打包与 maxRSS 单位未核验，不宣称支持已验证。

## 检查与测量

仓库内从已安装依赖开始，先构建 framework exports：

```sh
bun install --frozen-lockfile # landing 集成已将 Media 纳入唯一共享锁
bun run --filter @lenso/core build
bun run --filter @lenso/storage --filter @lenso/tasks build
bun run --filter @lenso/media build
bun run --filter @lenso/media typecheck
bun test packages/media/test
bun run --filter @lenso/media benchmark
cd packages/media && bun pm pack --destination <local-output-directory>
```

测试分层：native 真实解码；service SQLite + fake adapters 用可控 interleaving 检验 fencing/重试/撤权；integration 用真实 Files/local Storage、Miniflare/workerd D1 Tasks 和 Bun native；D1 store 在同一真实本地 binding 上验证 CAS；Workers graph build 拒绝 native/FS/SQLite 进入控制面。fault injection 包含 download、decode/transform、PUT 前后、登记前后和删除失败，不把 mocks 当作生产持久性证明。

测量脚本 `scripts/benchmark-native.ts` 使用固定确定性 1920×1080 RGBA PNG（2,024,732 bytes），320×320 cover WebP q80（27,036 bytes）。每种操作 warmup 后串行五次，并发 2 分三批共六次。单独一个相同协议 transform child 的 `Bun.Subprocess.resourceUsage().maxRSS` 在本机为 bytes，父进程 `process.resourceUsage().maxRSS` 为 KiB，再乘 1024。不是集群峰值或性能 SLA。

已实测环境：Bun 1.4.2、Darwin 27.0.0、arm64、Apple M2 Pro、sharp 0.35.5、libvips 8.18.7。以下为本次 `bun run --filter @lenso/media benchmark` 的真实输出：

| 项目                              | 结果                             |
| --------------------------------- | -------------------------------- |
| inspect，warmup 后 5 次           | 平均 74.13 ms，73.41–75.50 ms    |
| transform，warmup 后 5 次         | 平均 107.89 ms，105.57–109.58 ms |
| 6 次 transform，并发 2            | 合计 347.33 ms                   |
| 单 transform child high-water RSS | 102,285,312 bytes                |
| 测量父进程 high-water RSS         | 98,385,920 bytes                 |
| 编码输入 / 输出                   | 2,024,732 / 27,036 bytes         |

最终检查证据：

- `bun run --filter @lenso/media build`、`typecheck` 通过；dist 用单次 split build 保持 portable/native 共享错误类身份，编译产物回归测试验证这一点。
- `bun test packages/media/test`：65 passed、0 failed，328 assertions。覆盖图片策略、身份变化、并发/更高 fence、自动重试及 producer 重建、撤权、取消、残留恢复、登记模糊回应；已有 ready 请求也必须经过实时结果交付授权。
- `bun run build`：全仓 26 个 build tasks 成功，Workers 部分为 dry-run，不是部署。
- Notes 和 Workers 宿主 typecheck 通过；`bun test examples/notes/test/files.test.ts` 为 1 passed；`bun run --filter lenso-example-workers test:notes` 为 3 个真实 workerd/D1 测试 passed。
- `oxlint packages/media --deny-warnings` 和 `oxfmt --check packages/media` 通过。
- `bun pm pack --destination <scratch-directory>` 成功；解包到独立 consumer，借用本地已构建 framework/已安装 sharp，验证公共 exports、workspace 依赖改写、真实 PNG→WebP、共享错误身份和 Workers 控制面构建无 native/FS/SQLite。不是干净 registry 安装或发布批准。

初始包内开发阶段没有修改共享锁；landing 集成按 `AGENTS.md` 的集成 owner 政策在独立提交中同步 `bun.lock`，保留既有 workspace 和依赖解析。Media 的 sharp 0.35.5 与 Miniflare 已锁定的 sharp 0.35.4 分开解析，后续验证使用冻结安装。

未验证：生产 S3/R2/D1 权限与网络委派、PostgreSQL MediaStore（仅提供宿主可实现的 `MediaStore` 契约）、Linux/Windows/x64、严格 OS 资源 containment、远程复制/跨区域 failover、整套应用级 Auth 登录与生产撤权传播。没有发布、部署、付费、创建凭据或更改公共权限。

## 集成 owner 收口清单

- landing 集成已将 `packages/media`、sharp 0.35.5 和测试依赖纳入单一共享 Bun lock；后续依赖变更继续由集成 owner 同步并验证冻结安装。
- 原 Files factory 的最小补丁：queries 改为 `journal.queries`，现有 authorize 改为 `journal.authorizer({authorize: existingPolicy, authorizeDelivery})`，所有入口一致；不改 Files/Core 公共契约。
- DB：显式应用两张 Media 表的 migration，注入 borrowed store；所有物理文件继续使用现有 Files 表与删除 owner。
- Tasks：原队列显式注册 `createMediaTask`；producer/executor 使用相同 task name、queue identity、preset/fingerprint；executor 单独 Bun 部署，worker 启动晚于 Media 服务 ready。
- Auth：提供可信 scope、derive policy、live execute delegate 和独立受限 cleanup delegate。业务 JSON 不承载这四者。
- Workers：只导入 portable entries；网络/存储访问桥由宿主既有可信控制面收口，不将 native decoder 放入 Workers bundle。
- 配置/Manage：复用现有 plugin config 与 Operation declarations，应用 thin contextual async 方法绑定实际 Media instance，显式选择 CLI/MCP/Manage allowlist；不暴露 raw queue、execute、recover，不新建 UI/配置平台。本次没有更改 Core、Engine、Manage 或公共注册链路。
