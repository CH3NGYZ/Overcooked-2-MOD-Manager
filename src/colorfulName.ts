/**
 * 多彩名字生成：把逐字符样式编译成最短的富文本标签串。
 *
 * 游戏 MOD 支持两种内联标签：`<color=#rrggbb>` 和 `<size=数字>`，
 * 并且整段名字（UTF-16LE）不能超过 238 字节。逐字符直接套标签会迅速超限，
 * 所以这里用状态搜索合并相邻同样式字符，并在状态过多时按代价剪枝。
 */
export const DEFAULT_COLOR = '#000000'
export const DEFAULT_SIZE = 35
/** MOD 的显示名上限（UTF-16LE 字节，不含 BOM）。 */
export const MAX_NAME_BYTES = 238

export interface StyledChar {
  char: string
  color: string
  size: number
}

interface Unit {
  text: string
  /** null 表示默认色，不输出 color 标签。 */
  color: string | null
  /** null 表示默认字号，不输出 size 标签。 */
  size: string | null
}

interface Tag {
  type: 'color' | 'size'
  value: string
}

interface State {
  stack: Tag[]
  text: string
}

/** 相邻字符样式相同时合并成一个单元，减少状态数量。 */
export function toUnits(chars: StyledChar[]): Unit[] {
  const units: Unit[] = []
  for (const item of chars) {
    const color = item.color.toLowerCase() === DEFAULT_COLOR ? null : item.color.toLowerCase()
    const size = item.size === DEFAULT_SIZE ? null : String(item.size)
    const last = units[units.length - 1]
    if (last && last.color === color && last.size === size) {
      last.text += item.char
      continue
    }
    units.push({ text: item.char, color, size })
  }
  return units
}

function openTags(tags: Tag[]): string {
  return tags.map((tag) => `<${tag.type}=${tag.value}>`).join('')
}

function closeTags(tags: Tag[]): string {
  return tags
    .slice()
    .reverse()
    .map((tag) => `</${tag.type}>`)
    .join('')
}

function stackKey(stack: Tag[]): string {
  return stack.map((tag) => `${tag.type}=${tag.value}`).join('|')
}

function activeStyle(stack: Tag[]) {
  let color: string | null = null
  let size: string | null = null
  for (const tag of stack) {
    if (tag.type === 'color') color = tag.value === DEFAULT_COLOR ? null : tag.value
    else size = tag.value === String(DEFAULT_SIZE) ? null : tag.value
  }
  return { color, size }
}

/** 同时开关两个标签时，两种嵌套顺序都尝试，取最终更短的。 */
function tagOrders(tags: Tag[]): Tag[][] {
  if (tags.length < 2) return [tags]
  return [tags, [tags[1], tags[0]]]
}

function transitions(stack: Tag[], unit: Unit): State[] {
  const result: State[] = []
  for (let keep = 0; keep <= stack.length; keep += 1) {
    const base = stack.slice(0, keep)
    const close = closeTags(stack.slice(keep))
    const style = activeStyle(base)
    const toOpen: Tag[] = []
    if (style.color !== unit.color) toOpen.push({ type: 'color', value: unit.color ?? DEFAULT_COLOR })
    if (style.size !== unit.size) toOpen.push({ type: 'size', value: unit.size ?? String(DEFAULT_SIZE) })
    for (const order of tagOrders(toOpen)) {
      result.push({ stack: base.concat(order), text: close + openTags(order) })
    }
  }
  return result
}

const MAX_STATES = 64

/** 把逐字符样式编译成尽量短的标签串。 */
export function buildShortestRichText(chars: StyledChar[]): string {
  const units = toUnits(chars)
  if (!units.length) return ''

  let states: State[] = [{ stack: [], text: '' }]
  for (const unit of units) {
    const next = new Map<string, State>()
    for (const state of states) {
      for (const transition of transitions(state.stack, unit)) {
        const text = state.text + transition.text + unit.text
        const key = stackKey(transition.stack)
        const existing = next.get(key)
        if (!existing || text.length < existing.text.length) {
          next.set(key, { stack: transition.stack, text })
        }
      }
    }
    states = [...next.values()]
    if (states.length > MAX_STATES) {
      // 按「已生成文本 + 收尾标签」的代价保留最短的一批，避免状态爆炸
      states.sort((a, b) => (a.text.length + closeTags(a.stack).length) - (b.text.length + closeTags(b.stack).length))
      states = states.slice(0, MAX_STATES)
    }
  }

  let best = ''
  for (const state of states) {
    const text = state.text + closeTags(state.stack)
    if (!best || text.length < best.length) best = text
  }
  return best
}

/** MOD 按 UTF-16 代码单元计长度，每个单元 2 字节。 */
export function utf16LeBytes(text: string): number {
  return text.length * 2
}

export function isNameOverLimit(text: string): boolean {
  return utf16LeBytes(text) > MAX_NAME_BYTES
}
