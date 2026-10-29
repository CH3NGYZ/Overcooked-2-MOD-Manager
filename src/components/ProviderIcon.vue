<script setup lang="ts">
/**
 * 仓库图标：优先显示仓库头像，没有 / 加载失败时回落到平台图标。
 *
 * 头像地址是按平台的确定性规则拼出来的，不需要额外 API 请求：
 *   GitHub  `https://github.com/{owner}.png?size=64`
 *   Gitee   `https://gitee.com/{owner}.png?size=64`
 * 两者都会 302 跳到真实的图片地址，WebView 直接加载即可（CSP 已允许 https:）。
 *
 * 为什么必须保留回落：头像可能 404（组织改名、私有账号）、可能被网络环境
 * 挡住、也可能加载很慢。这三种情况下继续显示空白方块，比直接显示平台图标
 * 更糟 —— 用户会以为卡片坏了。所以加载失败**立刻**切回平台图标。
 */
import { computed, ref, watch } from 'vue'
import type { Provider } from '../types'

const props = defineProps<{
  provider: Provider
  /** 仓库所有者的用户名 / 组织名；留空则只显示平台图标。 */
  owner?: string
  size?: number
}>()

const box = computed(() => props.size ?? 30)

/**
 * 头像地址。
 *
 * `size=64` 是为高分屏：卡片上显示 30px，取大一点再缩下来才不会发虚。
 */
const avatarUrl = computed(() => {
  const owner = props.owner?.trim()
  if (!owner) return null
  // 用户名里不可能有斜杠，但仍做一次编码，避免拼出畸形 URL
  const encoded = encodeURIComponent(owner)
  return props.provider === 'gitee'
    ? `https://gitee.com/${encoded}.png?size=64`
    : `https://github.com/${encoded}.png?size=64`
})

/**
 * 头像是否可用。
 *
 * 初值为「有地址就试」，`@error` 时置 false。owner 变化时重置 —— 否则
 * 上一条仓库加载失败后，换到一条能加载的会一直显示平台图标。
 */
const avatarOk = ref(!!avatarUrl.value)
watch(avatarUrl, () => { avatarOk.value = !!avatarUrl.value })

/** GitHub 官方 mark（simple-icons 路径），跟随文字颜色以适配深浅主题。 */
const GITHUB =
  'M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12'

/** Gitee 官方 mark（simple-icons 路径）。 */
const GITEE =
  'M11.984 0A12 12 0 0 0 0 12a12 12 0 0 0 12 12 12 12 0 0 0 12-12A12 12 0 0 0 12 0a12 12 0 0 0-.016 0zm6.09 5.333c.328 0 .593.266.592.593v1.482a.594.594 0 0 1-.593.592H9.777c-.982 0-1.778.796-1.778 1.778v5.63c0 .327.266.592.593.592h5.63c.982 0 1.778-.796 1.778-1.778v-.296a.593.593 0 0 0-.592-.593h-4.15a.592.592 0 0 1-.592-.592v-1.482a.593.593 0 0 1 .593-.592h6.815c.327 0 .593.265.593.592v3.408a4 4 0 0 1-4 4H5.926a.593.593 0 0 1-.593-.593V9.778a4.444 4.444 0 0 1 4.445-4.444h8.205z'

const path = computed(() => (props.provider === 'gitee' ? GITEE : GITHUB))
const label = computed(() => (props.provider === 'gitee' ? 'Gitee' : 'GitHub'))
/** 有头像时用仓库 owner 做提示，比平台名更有用。 */
const title = computed(() => (avatarOk.value && props.owner ? props.owner : label.value))
</script>

<template>
  <span class="provider-icon" :class="`provider-icon--${provider}`" :style="{ width: `${box}px`, height: `${box}px` }"
    :title="title" :aria-label="title" role="img">
    <!--
      头像：懒加载 + 失败即回落。`referrerpolicy=no-referrer` 是因为
      GitHub 的头像 CDN 对部分来源会拒绝请求，去掉 Referer 最稳。
    -->
    <img v-if="avatarUrl && avatarOk" :src="avatarUrl" :alt="title" class="provider-avatar"
      loading="lazy" decoding="async" referrerpolicy="no-referrer" @error="avatarOk = false" />
    <svg v-else viewBox="0 0 24 24" :width="box * 0.62" :height="box * 0.62" aria-hidden="true">
      <path :d="path" fill="currentColor" />
    </svg>
  </span>
</template>

<style scoped>
.provider-icon {
  flex: 0 0 auto;
  display: grid;
  place-items: center;
  border-radius: 8px;
  border: 1px solid var(--oc2-row-line);
  background: var(--oc2-sunken);
  overflow: hidden;
}
.provider-avatar {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}
/* GitHub：黑白随主题；Gitee：品牌红 */
.provider-icon--github { color: var(--oc2-text-1); }
.provider-icon--gitee {
  color: #c71d23;
  background: color-mix(in srgb, #c71d23 14%, transparent);
  border-color: color-mix(in srgb, #c71d23 34%, transparent);
}
</style>
