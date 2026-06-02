import { Container, type SelectItem, SelectList, type SelectListLayoutOptions } from "@earendil-works/pi-tui";
import { getAvailableThemes, getSelectListTheme, setTheme } from "../theme/theme.ts";
import { DynamicBorder } from "./dynamic-border.ts";

const THEME_SELECT_LIST_LAYOUT: SelectListLayoutOptions = {
	minPrimaryColumnWidth: 12,
	maxPrimaryColumnWidth: 32,
};

/**
 * 主题选择器组件
 * 支持实时预览：光标移动时临时应用主题，取消时恢复原主题
 */
export class ThemeSelectorComponent extends Container {
	private selectList: SelectList;
	private onPreview: (themeName: string) => void;
	/** 打开选择器时保存的原始主题名，用于取消时恢复 */
	private originalThemeName: string;

	constructor(
		currentTheme: string,
		onSelect: (themeName: string) => void,
		onCancel: () => void,
		onPreview: (themeName: string) => void,
	) {
		super();
		this.onPreview = onPreview;
		// 记录打开时的主题，以便取消时恢复
		this.originalThemeName = currentTheme;

		// Get available themes and create select items
		const themes = getAvailableThemes();
		const themeItems: SelectItem[] = themes.map((name) => ({
			value: name,
			label: name,
			description: name === currentTheme ? "(current)" : undefined,
		}));

		// Add top border
		this.addChild(new DynamicBorder());

		// Create selector
		this.selectList = new SelectList(themeItems, 10, getSelectListTheme(), THEME_SELECT_LIST_LAYOUT);

		// Preselect current theme
		const currentIndex = themes.indexOf(currentTheme);
		if (currentIndex !== -1) {
			this.selectList.setSelectedIndex(currentIndex);
		}

		this.selectList.onSelect = (item) => {
			onSelect(item.value);
		};

		this.selectList.onCancel = () => {
			// 取消时恢复为打开选择器前的原始主题
			try {
				setTheme(this.originalThemeName);
			} catch {
				// 恢复失败时静默忽略，避免崩溃
			}
			onCancel();
		};

		this.selectList.onSelectionChange = (item) => {
			// 实时预览：光标移动时临时应用对应主题
			try {
				setTheme(item.value);
			} catch {
				// 预览加载失败时静默跳过，不影响选择器正常使用
			}
			this.onPreview(item.value);
		};

		this.addChild(this.selectList);

		// Add bottom border
		this.addChild(new DynamicBorder());
	}

	getSelectList(): SelectList {
		return this.selectList;
	}
}
