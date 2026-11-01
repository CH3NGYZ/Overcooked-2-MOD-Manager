import { ref } from 'vue'

/**
 * 两级卡片的展开状态：外层容器 + 容器里的条目。
 *
 * MOD 管理页与地图管理页结构相同，只是数据源不同：
 *  - MOD 管理页：多 DLL 包外壳 → 包内子 MOD / 随包依赖
 *  - 地图管理页：关卡集卡片 → 随包插件
 *
 * 只有一条硬规则，两页必须一致：**外层收起时，它名下的内层展开状态一起清掉**。
 * 否则收起再展开会看到上次留下的半开卡片 —— 那不是用户这次点开的样子，
 * 想收回还得再点一次内层卡片。
 *
 * 内层键用 NUL 拼在父键后面：文件系统名字里不可能出现 NUL，所以
 * `父键 + NUL + 子键` 不会与另一对父子撞键，清某个父的内层就是前缀过滤。
 */
export interface CardExpansionOptions {
  /**
   * 外层是否互斥（同一时刻只展开一个）。
   *
   * 地图管理页是单展开：一次只看一张集卡片，展开 B 会让 A 收起；
   * MOD 管理页允许多个包同时展开（用户要对照两个包的子 MOD）。
   */
  singleOuter?: boolean
}

/** 内层键的分隔符：NUL 写不进文件名，拼出来不会与别的父子对撞键。 */
const KEY_SEP = '\u0000'

export function useCardExpansion(options: CardExpansionOptions = {}) {
  const singleOuter = options.singleOuter === true
  /** 已展开的外层键；`singleOuter` 下最多一个。 */
  const outerKeys = ref<string[]>([])
  /** 已展开的内层键，形如 `父键 + NUL + 子键`。 */
  const innerKeys = ref<string[]>([])

  function isOuterOpen(key: string) {
    return outerKeys.value.includes(key)
  }

  function isInnerOpen(parent: string, child: string) {
    return innerKeys.value.includes(`${parent}${KEY_SEP}${child}`)
  }

  /**
   * 内层键里的子键部分。
   *
   * 页面渲染内层卡片时写的是原始字段（`data-dll-file` / `data-plugin-file`），
   * 键里那个 NUL 写不进 HTML 属性；点空白收起后要按子键找卡片滚回去，就得还原。
   */
  function childOf(key: string) {
    const at = key.lastIndexOf(KEY_SEP)
    return at < 0 ? key : key.slice(at + KEY_SEP.length)
  }

  /** 清掉某个外层名下的全部内层状态。 */
  function clearInnerOf(parent: string) {
    const prefix = `${parent}${KEY_SEP}`
    const next = innerKeys.value.filter((key) => !key.startsWith(prefix))
    if (next.length !== innerKeys.value.length) innerKeys.value = next
  }

  /**
   * 展开外层。`singleOuter` 下先把别的外层连同它们的内层一起收掉 ——
   * 被动收起的卡片也要清干净，否则下次展开它还带着上次的半开状态。
   */
  function openOuter(key: string) {
    if (singleOuter) {
      for (const other of outerKeys.value) {
        if (other !== key) clearInnerOf(other)
      }
      if (!(outerKeys.value.length === 1 && outerKeys.value[0] === key)) outerKeys.value = [key]
      return
    }
    if (!isOuterOpen(key)) outerKeys.value = [...outerKeys.value, key]
  }

  /** 收起外层，连带清掉它的内层：下次展开是一张全新的卡片。 */
  function closeOuter(key: string) {
    const next = outerKeys.value.filter((item) => item !== key)
    if (next.length !== outerKeys.value.length) outerKeys.value = next
    clearInnerOf(key)
  }

  /** 切换外层；返回切换后是否展开（页面据此决定往哪个方向滚）。 */
  function toggleOuter(key: string) {
    if (isOuterOpen(key)) {
      closeOuter(key)
      return false
    }
    openOuter(key)
    return true
  }

  function openInner(parent: string, child: string) {
    const key = `${parent}${KEY_SEP}${child}`
    if (!innerKeys.value.includes(key)) innerKeys.value = [...innerKeys.value, key]
  }

  function closeInner(parent: string, child: string) {
    const key = `${parent}${KEY_SEP}${child}`
    const next = innerKeys.value.filter((item) => item !== key)
    if (next.length !== innerKeys.value.length) innerKeys.value = next
  }

  /** 切换内层；返回切换后是否展开。 */
  function toggleInner(parent: string, child: string) {
    if (isInnerOpen(parent, child)) {
      closeInner(parent, child)
      return false
    }
    openInner(parent, child)
    return true
  }

  /** 收起全部外层与内层（地图管理页「点空白收起」用）。 */
  function closeAll() {
    if (outerKeys.value.length) outerKeys.value = []
    if (innerKeys.value.length) innerKeys.value = []
  }

  /** 只收起所有内层，外层保持（MOD 管理页点空白不收起包外壳）。 */
  function closeAllInner() {
    if (innerKeys.value.length) innerKeys.value = []
  }

  return {
    outerKeys,
    innerKeys,
    isOuterOpen,
    isInnerOpen,
    childOf,
    openOuter,
    closeOuter,
    toggleOuter,
    openInner,
    closeInner,
    toggleInner,
    closeAll,
    closeAllInner,
  }
}
