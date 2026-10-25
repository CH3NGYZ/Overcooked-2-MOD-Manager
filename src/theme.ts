import { darkTheme, type GlobalThemeOverrides } from 'naive-ui'

export { darkTheme }

/** 深色主题与产品色板：绿色为主操作色，蓝色仅用于品牌标识。 */
export const themeOverrides: GlobalThemeOverrides = {
  common: {
    fontFamily: "'Segoe UI', 'Microsoft YaHei', 'PingFang SC', sans-serif",
    borderRadius: '8px',
    bodyColor: '#131417',
    cardColor: '#1e2025',
    modalColor: '#1e2025',
    popoverColor: '#23252b',
    primaryColor: '#3cb89b',
    primaryColorHover: '#4fcfab',
    primaryColorPressed: '#2f9c83',
    primaryColorSuppl: '#3cb89b',
    successColor: '#3cb89b',
    warningColor: '#e6b86a',
    errorColor: '#ee8585',
    textColorBase: '#e8edf3',
    textColor1: '#e8edf3',
    textColor2: '#c3cdd8',
    textColor3: '#8d9bab',
    borderColor: '#2c3540',
    dividerColor: '#273341',
  },
  Card: {
    borderRadius: '10px',
  },
}
