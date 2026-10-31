import { computed, ref } from 'vue'

/**
 * 「源 → 分类 → 卡片」三层折叠树的唯一实现。
 *
 * MOD 安装页与地图下载页是同一个列表结构：第一层是源（`repository_sources`
 * 或地图源），第二层是分类（MOD 的 `group` / 地图的 `group`），第三层是卡片。
 * 两页以前各写一份分桶与展开逻辑，规则已经开始分叉（一页按显示名排序、
 * 一页按声明顺序）。这里把**规则**收成一份，两页只负责把各自的数据投影成
 * `SourceGroupEntry`，界面差异留在各自模板里。
 *
 * 三条不能各写一遍的规则：
 *
 * 1. **分桶键用原文，不用翻译名**。显示名随界面语言变化，拿它当 Map 键
 *    意味着切语言等于换一组桶、顺序跟着重排 —— 用户看到的是「改个语言，
 *    分组全乱了」。键只由原文决定（`groupKeyOf`），语言只影响 label。
 * 2. **顺序按声明顺序，不排序**。源顺序来自源文档/持久化配置的声明顺序；
 *    分类顺序来自条目首次出现的顺序。按名字排序同样会在切语言后跳。
 * 3. **默认全展开，搜索时强制全展开**。命中的卡片被折在别的分类里，
 *    等于搜索没生效。
 */
export interface SourceGroupEntry<T> {
  /** 源归属 ID；空字符串表示没有源归属（归入未归类桶）。 */
  sourceId: string
  /**
   * 源展示名（当前语言）。空字符串时按「`order` 里声明的昵称 → 源 ID」兜底，
   * 因为有的页面源名在源列表里、有的页面源名挂在条目上。
   */
  sourceLabel: string
  /** 分类原文（未规范化）。空 = 未分类。 */
  group: string
  /** 分类展示名（当前语言）。 */
  groupLabel: string
  item: T
}

/** 源声明顺序的一项（源 ID + 展示名）。 */
export interface SourceGroupOrder {
  id: string
  label: string
}

export interface SourceGroupLeaf<T> {
  /** 折叠项名称：`<源 ID>::<分类键>`，跨语言稳定。 */
  key: string
  label: string
  items: T[]
}

export interface SourceGroupSource<T> {
  /** 源 ID（折叠项名称，也是「刷新这个源」的入参）。 */
  id: string
  label: string
  count: number
  /** 「不分类」的源（如社区仓库）：只有一组且不显示分类标题。 */
  flat: boolean
  groups: SourceGroupLeaf<T>[]
}

export interface SourceGroupTreeOptions {
  /**
   * 源声明顺序。缺省（或不含某个源）时按条目首次出现的顺序补在后面：
   * 地图页没有持久化源树，顺序本来就只能从条目顺序读出来。
   */
  order?: SourceGroupOrder[]
  /** 没有源归属的条目的桶。 */
  ungrouped: { id: string; label: string }
  /** 该源只有一组且不显示分类标题（社区仓库）。 */
  flat?: (sourceId: string) => boolean
}

/** 未分类的桶键（与语言无关，不把「未分类」这类翻译文案写进键里）。 */
export const UNGROUPED_GROUP_KEY = '__ungrouped__'

/**
 * 分类键：原文规范化后加 `group:` 前缀。
 *
 * 与界面语言无关是硬要求，见文件头第 1 条。空分类归入 `__ungrouped__`。
 */
export function groupKeyOf(raw: string | null | undefined): string {
  const value = (raw ?? '').trim().toLowerCase()
  return value ? `group:${value}` : UNGROUPED_GROUP_KEY
}

/** 一层分类：保持条目首次出现的顺序，不排序。 */
function buildLeaves<T>(
  sourceId: string,
  entries: SourceGroupEntry<T>[],
): SourceGroupLeaf<T>[] {
  const leaves = new Map<string, SourceGroupLeaf<T>>()
  for (const entry of entries) {
    const key = groupKeyOf(entry.group)
    const existing = leaves.get(key)
    if (existing) {
      existing.items.push(entry.item)
      continue
    }
    leaves.set(key, { key: `${sourceId}::${key}`, label: entry.groupLabel, items: [entry.item] })
  }
  return [...leaves.values()]
}

/**
 * 把扁平条目排成「源 → 分类 → 卡片」两层节点（卡片本身就是第三层）。
 *
 * 源顺序：`order` 里声明且真有条目的源按声明顺序在前，其余源按条目首次
 * 出现顺序跟在后，未归类桶固定垫底 —— 与 MOD 安装页原本的行为一致。
 */
export function buildSourceGroupTree<T>(
  entries: SourceGroupEntry<T>[],
  options: SourceGroupTreeOptions,
): SourceGroupSource<T>[] {
  const flatOf = options.flat ?? (() => false)
  const ungroupedId = options.ungrouped.id
  const sourceIdOf = (entry: SourceGroupEntry<T>) => entry.sourceId || ungroupedId

  const buckets = new Map<string, SourceGroupEntry<T>[]>()
  for (const entry of entries) {
    const id = sourceIdOf(entry)
    const list = buckets.get(id)
    if (list) list.push(entry)
    else buckets.set(id, [entry])
  }

  const declaredLabels = new Map<string, string>()
  const ids: string[] = []
  const seen = new Set<string>()
  for (const declared of options.order ?? []) {
    if (declared.label) declaredLabels.set(declared.id, declared.label)
    if (!buckets.has(declared.id) || seen.has(declared.id)) continue
    seen.add(declared.id)
    ids.push(declared.id)
  }
  // 未在 order 里声明的源：按条目首次出现的顺序补在后面
  for (const id of buckets.keys()) {
    if (id === ungroupedId || seen.has(id)) continue
    seen.add(id)
    ids.push(id)
  }
  if (buckets.has(ungroupedId)) ids.push(ungroupedId)

  return ids.map((id) => {
    const items = buckets.get(id) ?? []
    /*
     * 源名的兜底顺序：声明昵称 → 条目上带的源名 → 源 ID。
     * 最后一级是给「有归属但没有名字」的源用的（老配置、源文档没写昵称），
     * 显示成 ID 总比把两张图混进一个叫「未归类」的桶里清楚。
     */
    const label = id === ungroupedId
      ? options.ungrouped.label
      : declaredLabels.get(id)
        ?? items.find((entry) => !!entry.sourceLabel)?.sourceLabel
        ?? id
    const flat = id !== ungroupedId && flatOf(id)
    const groups = flat
      ? [{ key: `${id}::flat`, label: '', items: items.map((entry) => entry.item) }]
      : buildLeaves(id, items)
    return { id, label, count: items.length, flat, groups }
  })
}

/** 所有源节点的折叠项名称。 */
export function sourceKeysOf<T>(tree: SourceGroupSource<T>[]): string[] {
  return tree.map((node) => node.id)
}

/** 所有分类节点的折叠项名称（不分类的源没有分类节点）。 */
export function groupKeysOf<T>(tree: SourceGroupSource<T>[]): string[] {
  return tree.flatMap((node) => (node.flat ? [] : node.groups.map((group) => group.key)))
}

/**
 * 折叠项该展开哪些：默认全展开，搜索时强制全展开。
 *
 * `open` 是用户手动收起的记录，`null` 表示「还没动过折叠」——此时默认全展开
 * （两页的用户预期都是「进来就能看见内容」，不是一层层点开）。
 *
 * 用 `null` 而不是空数组当「没动过」的哨兵：收起最后一个展开项时记录的正是
 * 空数组，若把它也当成「没动过」，最后一个源/分类就永远收不起来。地图安装页
 * 只有一两个源，那样等于第一层折叠完全失效。
 */
export function expandedNames(
  all: string[],
  open: string[] | null,
  searching: boolean,
): string[] {
  if (searching || open === null) return all
  return open
}

/**
 * 页面侧的组合式入口：树 + 折叠状态一次拿全。
 *
 * 入参是函数（而不是值）：树依赖响应式数据，Vue 的 `computed` 会自动追踪
 * 这些 getter 读到的 ref，页面不必自己再造一层 computed 去喂。
 */
export function useSourceGroupTree<T>(
  entries: () => SourceGroupEntry<T>[],
  options: () => SourceGroupTreeOptions,
  searching: () => boolean,
) {
  const tree = computed(() => buildSourceGroupTree(entries(), options()))
  /** 用户手动收起/展开的记录；`null` 表示还没交互过（用默认值：全展开）。 */
  const openSources = ref<string[] | null>(null)
  const openGroups = ref<string[] | null>(null)
  /*
   * 折叠状态的两个写入口。
   *
   * 模板里不直接给解构出来的 ref 赋值：`n-collapse` 的
   * `update:expanded-names` 需要的是「拿到就存」，写成函数比依赖模板
   * 赋值语义更稳，两个页面的写法也一致。
   */
  const setOpenSources = (names: string[]) => {
    openSources.value = names
  }
  const setOpenGroups = (names: string[]) => {
    openGroups.value = names
  }
  const sourcesExpanded = computed(() =>
    expandedNames(sourceKeysOf(tree.value), openSources.value, searching()),
  )
  const groupsExpanded = computed(() =>
    expandedNames(groupKeysOf(tree.value), openGroups.value, searching()),
  )
  return { tree, setOpenSources, setOpenGroups, sourcesExpanded, groupsExpanded }
}
