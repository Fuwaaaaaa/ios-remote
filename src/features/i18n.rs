use std::collections::HashMap;

/// Internationalization: multi-language UI string support.
pub struct I18n {
    current_lang: String,
    strings: HashMap<String, HashMap<String, String>>,
}

impl I18n {
    pub fn new(lang: &str) -> Self {
        let mut i18n = Self {
            current_lang: lang.to_string(),
            strings: HashMap::new(),
        };
        i18n.load_defaults();
        i18n
    }

    pub fn t<'a>(&'a self, key: &'a str) -> &'a str {
        self.strings
            .get(&self.current_lang)
            .and_then(|m| m.get(key))
            .or_else(|| self.strings.get("en").and_then(|m| m.get(key)))
            .map(|s| s.as_str())
            .unwrap_or(key)
    }

    pub fn set_language(&mut self, lang: &str) { self.current_lang = lang.to_string(); }
    pub fn current_language(&self) -> &str { &self.current_lang }

    pub fn available_languages(&self) -> Vec<&str> {
        self.strings.keys().map(|s| s.as_str()).collect()
    }

    fn load_defaults(&mut self) {
        // English
        let mut en = HashMap::new();
        en.insert("status.waiting".into(), "Waiting for iPhone...".into());
        en.insert("status.connected".into(), "Connected".into());
        en.insert("status.disconnected".into(), "Disconnected".into());
        en.insert("action.screenshot".into(), "Screenshot".into());
        en.insert("action.record".into(), "Record".into());
        en.insert("action.stop".into(), "Stop".into());
        en.insert("action.ocr".into(), "Extract Text".into());
        en.insert("action.quit".into(), "Quit".into());
        en.insert("settings.title".into(), "Settings".into());
        en.insert("settings.language".into(), "Language".into());
        en.insert("settings.theme".into(), "Theme".into());
        en.insert("overlay.fps".into(), "FPS".into());
        en.insert("overlay.latency".into(), "Latency".into());
        self.strings.insert("en".into(), en);

        // 日本語
        let mut ja = HashMap::new();
        ja.insert("status.waiting".into(), "iPhoneの接続を待っています...".into());
        ja.insert("status.connected".into(), "接続済み".into());
        ja.insert("status.disconnected".into(), "切断".into());
        ja.insert("action.screenshot".into(), "スクリーンショット".into());
        ja.insert("action.record".into(), "録画".into());
        ja.insert("action.stop".into(), "停止".into());
        ja.insert("action.ocr".into(), "テキスト抽出".into());
        ja.insert("action.quit".into(), "終了".into());
        ja.insert("settings.title".into(), "設定".into());
        ja.insert("settings.language".into(), "言語".into());
        ja.insert("settings.theme".into(), "テーマ".into());
        ja.insert("overlay.fps".into(), "FPS".into());
        ja.insert("overlay.latency".into(), "遅延".into());
        self.strings.insert("ja".into(), ja);

        // 中文
        let mut zh = HashMap::new();
        zh.insert("status.waiting".into(), "等待iPhone连接...".into());
        zh.insert("status.connected".into(), "已连接".into());
        zh.insert("status.disconnected".into(), "已断开".into());
        zh.insert("action.screenshot".into(), "截图".into());
        zh.insert("action.record".into(), "录制".into());
        zh.insert("action.stop".into(), "停止".into());
        zh.insert("action.ocr".into(), "提取文字".into());
        zh.insert("action.quit".into(), "退出".into());
        self.strings.insert("zh".into(), zh);

        // 한국어
        let mut ko = HashMap::new();
        ko.insert("status.waiting".into(), "iPhone 연결 대기 중...".into());
        ko.insert("status.connected".into(), "연결됨".into());
        ko.insert("status.disconnected".into(), "연결 끊김".into());
        ko.insert("action.screenshot".into(), "스크린샷".into());
        ko.insert("action.record".into(), "녹화".into());
        ko.insert("action.stop".into(), "중지".into());
        ko.insert("action.ocr".into(), "텍스트 추출".into());
        ko.insert("action.quit".into(), "종료".into());
        self.strings.insert("ko".into(), ko);
    }
}
