# Runtime Modes

## 目的

運用時に mode の意味と遷移条件を一意に解釈できるようにします。

## mode 一覧

| mode | outbound | 説明 |
|---|---|---|
| normal | react/send | 通常運転 |
| observe_only | なし | 監視専用、送信停止 |
| react_only | react のみ | send を react へ縮退 |
| audit_only | なし | 解析失敗増加時の保護停止 |
| full_disable | なし | 緊急停止 |

## 遷移トリガ

- emergency kill switch -> full_disable
- mode override -> 指定 mode
- session budget low -> react_only
- breaker open -> observe_only
- sandbox failure spike -> audit_only
- trigger 消失 -> normal

## 運用操作

- 一時的な強制 mode:
  - `OO_MODE_OVERRIDE=observe-only` など
- 緊急停止:
  - `OO_EMERGENCY_KILL_SWITCH=true`

## 監視ポイント

- mode transitions の増加
- `suppress_reason=mode_restricted` の増加
