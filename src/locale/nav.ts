import type { DomainMessages } from './types'

/**
 * nav 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const nav: DomainMessages = {
  // 总览
  overview: { 'zh-CN': '总览', 'zh-TW': '總覽', en: 'Overview', ko: '개요', ja: '概要', de: "Übersicht", fr: "Vue d’ensemble", it: "Panoramica", 'es-ES': "Resumen", ru: "Обзор", pl: "Przegląd", 'pt-BR': "Visão geral" },
  // 框架管理
  framework: { 'zh-CN': '框架管理', 'zh-TW': '框架管理', en: 'Framework', ko: '프레임워크', ja: 'フレームワーク', de: "Frameworks", fr: "Frameworks", it: "Framework", 'es-ES': "Entornos", ru: "Фреймворки", pl: "Frameworki", 'pt-BR': "Estruturas" },
  // MOD 安装
  modInstall: { 'zh-CN': 'MOD 安装', 'zh-TW': 'MOD 安裝', en: 'MOD Install', ko: 'MOD 설치', ja: 'MOD インストール', de: "Mods installieren", fr: "Installer des mods", it: "Installa mod", 'es-ES': "Instalar mods", ru: "Установка модов", pl: "Instalacja modów", 'pt-BR': "Instalar mods" },
  // MOD 管理
  modManager: { 'zh-CN': 'MOD 管理', 'zh-TW': 'MOD 管理', en: 'MOD Manager', ko: 'MOD 관리', ja: 'MOD マネージャー', de: "Mod-Verwaltung", fr: "Gestion des mods", it: "Gestione mod", 'es-ES': "Gestionar mods", ru: "Управление модами", pl: "Zarządzanie modami", 'pt-BR': "Gerenciar mods" },
  // 工具
  tools: { 'zh-CN': '工具', 'zh-TW': '工具', en: 'Tools', ko: '도구', ja: 'ツール', de: "Werkzeuge", fr: "Outils", it: "Strumenti", 'es-ES': "Herramientas", ru: "Инструменты", pl: "Narzędzia", 'pt-BR': "Ferramentas" },
  // 设置
  settings: { 'zh-CN': '设置', 'zh-TW': '設定', en: 'Settings', ko: '설정', ja: '設定', de: "Einstellungen", fr: "Paramètres", it: "Impostazioni", 'es-ES': "Ajustes", ru: "Настройки", pl: "Ustawienia", 'pt-BR': "Configurações" },
  // 快捷管理 《胡闹厨房2》的MOD
  quicklyManageModsForOvercooked2: { 'zh-CN': '快捷管理 《胡闹厨房2》的MOD', 'zh-TW': '快捷管理《胡鬧廚房2》的MOD', en: 'Quickly manage MODs for Overcooked! 2', ko: '《오버쿡드! 2》 MOD를 빠르게 관리', ja: '『オーバークック2』のMODをすばやく管理', de: "MODs für Overcooked! 2 schnell verwalten", fr: "Gérer rapidement les MOD pour Overcooked! 2", it: "Gestione rapida dei MOD per Overcooked! 2", 'es-ES': "Administrar rápidamente MODs para Overcooked! 2", ru: "Быстрый менеджмент для Overcooked! 2", pl: "Szybkie zarządzanie MOD dla Overcooked! 2", 'pt-BR': "Gerenciar rapidamente MODs para Overcooked! 2" },
  // OC2 模组管理器
  oc2ModManager: { 'zh-CN': 'OC2 模组管理器', 'zh-TW': 'OC2 模組管理器', en: 'OC2 Mod Manager', ko: 'OC2 모드 관리자', ja: 'OC2 MODマネージャー', de: "OC2 Mod-Manager", fr: "Gestionnaire de mods OC2", it: "Gestore mod OC2", 'es-ES': "Gestor de mods de OC2", ru: "Менеджер модов OC2", pl: "Menedżer modów OC2", 'pt-BR': "Gerenciador de mods OC2" },
  // 界面语言
  language: { 'zh-CN': '界面语言', 'zh-TW': '介面語言', en: 'Language', ko: '인터페이스 언어', ja: '表示言語', de: "Sprache", fr: "Langue", it: "Lingua", 'es-ES': "Idioma", ru: "Язык", pl: "Język", 'pt-BR': "Idioma" },
  // 界面缩放
  uiScale: { 'zh-CN': '界面缩放', 'zh-TW': '介面縮放', en: 'UI scale', ko: '인터페이스 배율', ja: '表示倍率', de: "UI-Skala", fr: "Taux d'assurance-chômage", it: "Scala dell'interfaccia utente", 'es-ES': "Escala UI", ru: "Шкала UI", pl: "Skala interfejsu użytkownika", 'pt-BR': "Escala de IU" },
  // 主题模式
  theme: { 'zh-CN': '主题模式', 'zh-TW': '主題模式', en: 'Theme', ko: '테마', ja: 'テーマ', de: "Thema", fr: "Thème", it: "Tema", 'es-ES': "Tema", ru: "Тема", pl: "Motyw", 'pt-BR': "Tema" },
  // 深色
  dark: { 'zh-CN': '深色', 'zh-TW': '深色', en: 'Dark', ko: '다크', ja: 'ダーク', de: "Dunkel", fr: "Ténèbre", it: "Oscuro", 'es-ES': "Oscuro", ru: "темный", pl: "Ciemny", 'pt-BR': "Escuro" },
  // 浅色
  light: { 'zh-CN': '浅色', 'zh-TW': '淺色', en: 'Light', ko: '라이트', ja: 'ライト', de: "Licht", fr: "Lumière", it: "Luce", 'es-ES': "Luz", ru: "Свет", pl: "Światło", 'pt-BR': "Luz" },
  // 跟随系统外观（深/浅由系统决定）
  auto: { 'zh-CN': '跟随系统', 'zh-TW': '跟隨系統', en: 'Follow system', ko: '시스템 설정 따르기', ja: 'システムに合わせる', de: "Folgesystem", fr: "Suivre le système", it: "Segui il sistema", 'es-ES': "Sistema de seguimiento", ru: "Система контроля", pl: "Podążaj za systemem", 'pt-BR': "Seguir o sistema" },
  // 主题色
  accent: { 'zh-CN': '主题色', 'zh-TW': '主題色', en: 'Accent', ko: '테마 색', ja: 'テーマカラー', de: "Aczent", fr: "Accents", it: "Accente", 'es-ES': "Accentivo", ru: "Акцент", pl: "Akcent", 'pt-BR': "Acento" },
  // 中性
  neutral: { 'zh-CN': '中性', 'zh-TW': '中性', en: 'Neutral', ko: '뉴트럴', ja: 'ニュートラル', de: "Neutrale", fr: "Neutre", it: "Neutrale", 'es-ES': "Neutra", ru: "нейтральный", pl: "Neutralny", 'pt-BR': "Neutro" },
  // 板岩
  slate: { 'zh-CN': '板岩', 'zh-TW': '板岩', en: 'Slate', ko: '슬레이트', ja: 'スレート', de: "Schiefer", fr: "Ardoise", it: "Ardesia", 'es-ES': "Pizarra", ru: "Сланцевый", pl: "Łupkowy", 'pt-BR': "Ardósia" },
  // 灰色
  gray: { 'zh-CN': '灰色', 'zh-TW': '灰色', en: 'Gray', ko: '그레이', ja: 'グレー', de: "Grau", fr: "Gris", it: "Grigio", 'es-ES': "Gris", ru: "серый", pl: "Szary", 'pt-BR': "Cinza" },
  // 玫红
  rose: { 'zh-CN': '玫红', 'zh-TW': '玫紅', en: 'Rose', ko: '로즈', ja: 'ローズ', de: "Rosa", fr: "Rose", it: "Rosa", 'es-ES': "Rosa", ru: "Розовый", pl: "Różowy", 'pt-BR': "Rosa" },
  // 蓝色
  blue: { 'zh-CN': '蓝色', 'zh-TW': '藍色', en: 'Blue', ko: '블루', ja: 'ブルー', de: "Blau", fr: "Bleu", it: "Blu blu", 'es-ES': "Azul", ru: "голубой", pl: "Niebieski", 'pt-BR': "Azul" },
  // 绿色
  green: { 'zh-CN': '绿色', 'zh-TW': '綠色', en: 'Green', ko: '그린', ja: 'グリーン', de: "Grüngrün", fr: "Vert", it: "Verde", 'es-ES': "Verde", ru: "Зеленый", pl: "Zielony", 'pt-BR': "Verde" },
  // 紫色
  purple: { 'zh-CN': '紫色', 'zh-TW': '紫色', en: 'Purple', ko: '퍼플', ja: 'パープル', de: "Purpur", fr: "Pourpre", it: "Viola", 'es-ES': "Púrpura", ru: "фиолетовый", pl: "Fioletowy", 'pt-BR': "Roxo" },
  // 红色
  red: { 'zh-CN': '红色', 'zh-TW': '紅色', en: 'Red', ko: '레드', ja: 'レッド', de: "rot", fr: "Rouge", it: "Rosso", 'es-ES': "Rojo", ru: "красный", pl: "Czerwony", 'pt-BR': "Vermelho" },
  // 橙色
  orange: { 'zh-CN': '橙色', 'zh-TW': '橙色', en: 'Orange', ko: '오렌지', ja: 'オレンジ', de: "Orangen", fr: "Oranges", it: "Arancione", 'es-ES': "Nacido", ru: "Оранжевый", pl: "Pomarańczowy", 'pt-BR': "Laranja" },
  // 琥珀
  amber: { 'zh-CN': '琥珀', 'zh-TW': '琥珀', en: 'Amber', ko: '앰버', ja: 'アンバー', de: "Bernstein", fr: "Ambre", it: "Ambra", 'es-ES': "Ámbar", ru: "Янтарный", pl: "Bursztynowy", 'pt-BR': "Âmbar" },
  // 青色
  teal: { 'zh-CN': '青色', 'zh-TW': '青色', en: 'Teal', ko: '틸', ja: 'ティール', de: "Petrol", fr: "Bleu-vert", it: "Verde petrolio", 'es-ES': "Verde azulado", ru: "Сине-зелёный", pl: "Morski", 'pt-BR': "Verde-azulado" },
  // 最小化
  minimize: { 'zh-CN': '最小化', 'zh-TW': '最小化', en: 'Minimize', ko: '최소화', ja: '最小化', de: "Minimierung", fr: "Minimiser", it: "Minimizzare", 'es-ES': "Minimización", ru: "Минимизировать", pl: "Minimalizuj", 'pt-BR': "Minimizar" },
  // 最大化
  maximize: { 'zh-CN': '最大化', 'zh-TW': '最大化', en: 'Maximize', ko: '최대화', ja: '最大化', de: "Maximieren", fr: "Maximiser", it: "Massimizzare", 'es-ES': "Maximizar", ru: "Максимизировать", pl: "Maksymalizuj", 'pt-BR': "Maximizar" },
  // 还原
  restore: { 'zh-CN': '还原', 'zh-TW': '還原', en: 'Restore', ko: '복원', ja: '元に戻す', de: "Wiederherstellen", fr: "Restaurer", it: "Ripristina", 'es-ES': "Restaurar", ru: "Восстановить", pl: "Przywróć", 'pt-BR': "Restaurar" },
  // 关闭
  close: { 'zh-CN': '关闭', 'zh-TW': '關閉', en: 'Close', ko: '닫기', ja: '閉じる', de: "Schließen", fr: "Fermer", it: "Chiudi", 'es-ES': "Cerrar", ru: "Закрыть", pl: "Zamknij", 'pt-BR': "Fechar" },
}
