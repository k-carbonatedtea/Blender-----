import re
with open("src/ui/app.rs", "r", encoding="utf-8") as f:
    content = f.read()
pattern = r"        // 添加导出基础MO文件的按钮和说明\n        ui.horizontal\(\\|ui\\| \{\n            if ui.button\(\"导出基础文件\"\).clicked\(\) \{\n                self.export_base_mo_file\(\);\n            \}\n            ui.label\(\"\(将当前的基础MO文件导出为独立文件，不做任何合并\)\"\);\n        \}\);\n\n"
new_content = content.replace(pattern, "")
with open("src/ui/app.rs", "w", encoding="utf-8") as f:
    f.write(new_content)
print("文件修改完成!")
