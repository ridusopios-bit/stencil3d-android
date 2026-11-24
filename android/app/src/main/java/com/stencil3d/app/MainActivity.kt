package com.stencil3d.app

import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.*
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import kotlinx.coroutines.*
import org.json.JSONObject

class MainActivity : AppCompatActivity() {
    private val scope = CoroutineScope(Dispatchers.Main + SupervisorJob())
    private var processJob: Job? = null
    private var hasImage = false
    private var hasModel = false
    private var is3dTab = false
    private var meshDirty = true
    private var pendingExportKind = "step"

    // Настройки (соответствуют Rust StencilParams)
    private var bgMode = "auto_border"
    private var bgTolerance = 32
    private var invert = false
    private var contrast = 1.15f
    private var smoothStrength = 2
    private var styleMode = "art_hybrid"
    private var shadowThreshold = 92
    private var edgeSensitivity = 55
    private var lineThicknessMm = 1.8f
    private var strengthenOuter = true
    private var outerContourMm = 2.0f
    private var islandMode = "bridge"
    private var bridgeWidthMm = 2.0f
    private var maxBridgeLenMm = 18.0f
    private var minIslandMm2 = 6.0f
    private var minHoleMm2 = 3.5f
    private var wallOffsetMm = 0.0f
    private var pruneSpursMm = 0.30f
    private var simplifyTolMm = 0.18f
    private var targetHeightMm = 180.0f
    private var thicknessMm = 4.0f
    private var standMode = "slotted"
    private var addBottomBar = true
    private var standDepthMm = 32.0f
    private var standHeightMm = 10.0f
    private var slotClearanceMm = 0.25f

    private lateinit var preview2D: Preview2DView
    private lateinit var preview3D: Preview3DView
    private lateinit var lblAudit: TextView
    private lateinit var btn2d: Button
    private lateinit var btn3d: Button
    private lateinit var btnExportStep: Button
    private lateinit var btnExportStl: Button
    private lateinit var btnExportSvg: Button
    private lateinit var settingsScroll: ScrollView

    private val pickPhotoLauncher = registerForActivityResult(
        ActivityResultContracts.GetContent()
    ) { uri: Uri? ->
        if (uri != null) loadSelectedImage(uri)
    }

    private val saveFileLauncher = registerForActivityResult(
        ActivityResultContracts.CreateDocument("application/octet-stream")
    ) { uri: Uri? ->
        if (uri != null) performExport(uri, pendingExportKind)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(buildRootLayout())
    }

    override fun onDestroy() {
        super.onDestroy()
        scope.cancel()
    }

    private fun dp(v: Int): Int = (v * resources.displayMetrics.density).toInt()

    private fun roundedBg(fillHex: String, strokeHex: String? = null, radiusDp: Int = 8): GradientDrawable {
        return GradientDrawable().apply {
            setColor(Color.parseColor(fillHex))
            cornerRadius = dp(radiusDp).toFloat()
            if (strokeHex != null) setStroke(dp(1), Color.parseColor(strokeHex))
        }
    }

    private fun makeBtn(label: String, bgHex: String, onClick: (Button) -> Unit): Button {
        return Button(this).apply {
            text = label
            isAllCaps = false
            textSize = 12.5f
            setTextColor(Color.WHITE)
            background = roundedBg(bgHex, "#3E475B", 7)
            setPadding(dp(8), 0, dp(8), 0)
            setOnClickListener { onClick(this) }
        }
    }

    private fun buildRootLayout(): View {
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#1B1E26"))
            setPadding(dp(8), dp(8), dp(8), dp(8))
        }

        // 1. Верхняя панель действий
        val topBar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        val btnOpen = makeBtn("📂 Фото", "#2A303E") { pickPhotoLauncher.launch("image/*") }
        btnExportSvg = makeBtn("SVG", "#2A303E") {
            pendingExportKind = "svg"
            saveFileLauncher.launch("stencil_contour.svg")
        }.apply { isEnabled = false; alpha = 0.45f }
        btnExportStl = makeBtn("STL", "#2A303E") {
            pendingExportKind = "stl"
            saveFileLauncher.launch("stencil_3d.stl")
        }.apply { isEnabled = false; alpha = 0.45f }
        btnExportStep = makeBtn("💾 STEP", "#169C5D") {
            pendingExportKind = "step"
            saveFileLauncher.launch("stencil_3d.step")
        }.apply { isEnabled = false; alpha = 0.45f }

        topBar.addView(btnOpen, LinearLayout.LayoutParams(0, dp(42), 1.15f).apply { marginEnd = dp(5) })
        topBar.addView(btnExportSvg, LinearLayout.LayoutParams(0, dp(42), 0.62f).apply { marginEnd = dp(5) })
        topBar.addView(btnExportStl, LinearLayout.LayoutParams(0, dp(42), 0.62f).apply { marginEnd = dp(5) })
        topBar.addView(btnExportStep, LinearLayout.LayoutParams(0, dp(42), 1.15f))
        root.addView(topBar)

        // 2. Баннер проверки топологии
        lblAudit = TextView(this).apply {
            text = "Выберите фото — здесь появится проверка замкнутости и островков"
            textSize = 11.5f
            setTextColor(Color.parseColor("#C2CDE0"))
            setPadding(dp(10), dp(6), dp(10), dp(6))
            background = roundedBg("#232A38", "#384357")
        }
        root.addView(
            lblAudit,
            LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                LinearLayout.LayoutParams.WRAP_CONTENT
            ).apply { topMargin = dp(6); bottomMargin = dp(6) }
        )

        // 3. Панель переключения 2D / 3D / режимов предосмотра
        val viewBar = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        btn2d = makeBtn("2D Вид", "#2E5B88") { switchPreviewTab(false) }
        btn3d = makeBtn("3D Модель", "#242935") { switchPreviewTab(true) }
        val btnMode = makeBtn("Трафарет", "#242935") { btn ->
            preview2D.viewMode = when (preview2D.viewMode) {
                "stencil" -> { btn.text = "Наложение"; "overlay" }
                "overlay" -> { btn.text = "CAD-линии"; "cad" }
                else -> { btn.text = "Трафарет"; "stencil" }
            }
            preview2D.invalidate()
        }
        val btnFull = makeBtn("⛶", "#242935") {
            settingsScroll.visibility =
                if (settingsScroll.visibility == View.VISIBLE) View.GONE else View.VISIBLE
        }
        viewBar.addView(btn2d, LinearLayout.LayoutParams(0, dp(36), 0.9f).apply { marginEnd = dp(4) })
        viewBar.addView(btn3d, LinearLayout.LayoutParams(0, dp(36), 0.9f).apply { marginEnd = dp(4) })
        viewBar.addView(btnMode, LinearLayout.LayoutParams(0, dp(36), 1.15f).apply { marginEnd = dp(4) })
        viewBar.addView(btnFull, LinearLayout.LayoutParams(dp(42), dp(36)))
        root.addView(viewBar)

        // 4. Область предосмотра (2D + 3D)
        val previewFrame = FrameLayout(this).apply {
            background = roundedBg("#14171E", "#303746", 8)
            clipToOutline = true
        }
        preview2D = Preview2DView(this)
        preview3D = Preview3DView(this).apply { visibility = View.GONE }
        previewFrame.addView(
            preview2D,
            FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT)
        )
        previewFrame.addView(
            preview3D,
            FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT)
        )
        root.addView(
            previewFrame,
            LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1.15f).apply {
                topMargin = dp(6)
                bottomMargin = dp(6)
            }
        )

        // 5. Нижняя прокручиваемая панель настроек
        settingsScroll = ScrollView(this)
        val settingsCol = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(4), dp(2), dp(4), dp(18))
        }

        // Переключатели подсветки в предосмотре
        val visRow = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        visRow.addView(makeCheck("Мосты", true) { preview2D.showBridges = it; preview2D.invalidate() },
            LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        visRow.addView(makeCheck("Удалённое", true) { preview2D.showRemoved = it; preview2D.invalidate() },
            LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        visRow.addView(makeCheck("Подставка", true) { preview2D.showStand = it; preview2D.invalidate() },
            LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        settingsCol.addView(visRow)

        // Раздел 1: Изображение и стиль
        settingsCol.addView(makeSectionTitle("1. Изображение и стиль силуэта"))
        settingsCol.addView(
            makeSpinnerRow(
                "Стиль:",
                listOf("Арт-силуэт (как на примере)", "Адаптивный трафарет", "Ч/Б порог", "Только контуры"),
                0
            ) { idx ->
                styleMode = listOf("art_hybrid", "adaptive", "threshold", "edges_only")[idx]
                scheduleReprocess()
            }
        )
        settingsCol.addView(
            makeSpinnerRow("Удаление фона:", listOf("Авто по краям", "Не удалять фон"), 0) { idx ->
                bgMode = if (idx == 0) "auto_border" else "none"
                scheduleReprocess()
            }
        )
        settingsCol.addView(makeSliderRow("Допуск фона", 5, 100, bgTolerance, "") { bgTolerance = it; scheduleReprocess() })
        settingsCol.addView(makeSliderRow("Порог теней", 10, 245, shadowThreshold, "") { shadowThreshold = it; scheduleReprocess() })
        settingsCol.addView(makeSliderRow("Детализация линий", 0, 100, edgeSensitivity, "%") { edgeSensitivity = it; scheduleReprocess() })
        settingsCol.addView(makeSliderRow("Толщина линий", 8, 50, 18, " мм", 10f) { lineThicknessMm = it / 10f; scheduleReprocess() })
        settingsCol.addView(makeSliderRow("Контраст", 50, 250, 115, "", 100f) { contrast = it / 100f; scheduleReprocess() })
        settingsCol.addView(makeSliderRow("Сглаживание шума", 0, 4, smoothStrength, "") { smoothStrength = it; scheduleReprocess() })
        settingsCol.addView(makeCheck("Усиливать замкнутый внешний контур", true) { strengthenOuter = it; scheduleReprocess() })
        settingsCol.addView(makeCheck("Инвертировать (чёрное ↔ белое)", false) { invert = it; scheduleReprocess() })

        // Раздел 2: Защита для 3D-печати
        settingsCol.addView(makeSectionTitle("2. Защита для 3D-печати (без островков)"))
        settingsCol.addView(
            makeSpinnerRow("Островки внутри:", listOf("Авто-мосты (перемычки)", "Удалять все островки"), 0) { idx ->
                islandMode = if (idx == 0) "bridge" else "remove"
                scheduleReprocess()
            }
        )
        settingsCol.addView(makeSliderRow("Ширина перемычки", 10, 50, 20, " мм", 10f) { bridgeWidthMm = it / 10f; scheduleReprocess() })
        settingsCol.addView(makeSliderRow("Макс. длина моста", 2, 80, 18, " мм") { maxBridgeLenMm = it.toFloat(); scheduleReprocess() })
        settingsCol.addView(makeSliderRow("Мин. площадь островка", 5, 400, 60, " мм²", 10f) { minIslandMm2 = it / 10f; scheduleReprocess() })
        settingsCol.addView(makeSliderRow("Мин. площадь выреза", 0, 250, 35, " мм²", 10f) { minHoleMm2 = it / 10f; scheduleReprocess() })
        settingsCol.addView(makeSliderRow("Утолщение стенок", -10, 20, 0, " мм", 10f) { wallOffsetMm = it / 10f; scheduleReprocess() })
        settingsCol.addView(makeSliderRow("Убрать тонкие шипы", 0, 15, 3, " мм", 10f) { pruneSpursMm = it / 10f; scheduleReprocess() })
        settingsCol.addView(makeSliderRow("Упрощение STEP", 5, 80, 18, " мм", 100f) { simplifyTolMm = it / 100f; scheduleReprocess() })

        // Раздел 3: Размеры и подставка
        settingsCol.addView(makeSectionTitle("3. Размеры и подставка"))
        settingsCol.addView(
            makeSpinnerRow(
                "Подставка:",
                listOf("Отдельная с пазом (как на примере)", "Монолитная стоя", "Без подставки"),
                0
            ) { idx ->
                standMode = listOf("slotted", "assembled", "none")[idx]
                scheduleReprocess()
            }
        )
        settingsCol.addView(makeCheck("Нижняя планка в паз (связывает опоры)", true) { addBottomBar = it; scheduleReprocess() })
        settingsCol.addView(makeSliderRow("Высота фигуры", 40, 350, 180, " мм") { targetHeightMm = it.toFloat(); scheduleReprocess() })
        settingsCol.addView(makeSliderRow("Толщина модели", 15, 150, 40, " мм", 10f) { thicknessMm = it / 10f; scheduleReprocess() })
        settingsCol.addView(makeSliderRow("Глубина подставки", 15, 70, 32, " мм") { standDepthMm = it.toFloat(); scheduleReprocess() })
        settingsCol.addView(makeSliderRow("Высота подставки", 6, 22, 10, " мм") { standHeightMm = it.toFloat(); scheduleReprocess() })
        settingsCol.addView(makeSliderRow("Зазор паза", 5, 80, 25, " мм", 100f) { slotClearanceMm = it / 100f; scheduleReprocess() })

        settingsScroll.addView(settingsCol)
        root.addView(
            settingsScroll,
            LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1.0f)
        )

        return root
    }

    private fun makeSectionTitle(title: String): TextView {
        return TextView(this).apply {
            text = title
            textSize = 13f
            setTextColor(Color.parseColor("#8CC2FF"))
            setPadding(dp(2), dp(10), dp(2), dp(4))
        }
    }

    private fun makeCheck(label: String, checked: Boolean, onChange: (Boolean) -> Unit): CheckBox {
        return CheckBox(this).apply {
            text = label
            isChecked = checked
            textSize = 12.5f
            setTextColor(Color.parseColor("#E2E6EF"))
            setOnCheckedChangeListener { _, isChecked -> onChange(isChecked) }
        }
    }

    private fun makeSpinnerRow(
        label: String,
        items: List<String>,
        initial: Int,
        onSelect: (Int) -> Unit
    ): View {
        val row = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(0, dp(3), 0, dp(3))
        }
        val tv = TextView(this).apply {
            text = label
            textSize = 12.5f
            setTextColor(Color.parseColor("#D4DAE6"))
        }
        val sp = Spinner(this).apply {
            adapter = ArrayAdapter(this@MainActivity, android.R.layout.simple_spinner_dropdown_item, items)
            setSelection(initial, false)
            onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
                override fun onItemSelected(p: AdapterView<*>?, v: View?, pos: Int, id: Long) = onSelect(pos)
                override fun onNothingSelected(p: AdapterView<*>?) {}
            }
        }
        row.addView(tv, LinearLayout.LayoutParams(dp(120), LinearLayout.LayoutParams.WRAP_CONTENT))
        row.addView(sp, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        return row
    }

    private fun makeSliderRow(
        label: String,
        minVal: Int,
        maxVal: Int,
        initVal: Int,
        suffix: String,
        divisor: Float = 1f,
        onChange: (Int) -> Unit
    ): View {
        val box = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(0, dp(2), 0, dp(2))
        }
        val top = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        val tvTitle = TextView(this).apply {
            text = label
            textSize = 12f
            setTextColor(Color.parseColor("#CFD6E3"))
        }
        fun fmt(v: Int): String = when {
            divisor == 1f -> "$v$suffix"
            divisor >= 100f -> String.format("%.2f%s", v / divisor, suffix)
            else -> String.format("%.1f%s", v / divisor, suffix)
        }
        val tvVal = TextView(this).apply {
            text = fmt(initVal)
            textSize = 12f
            setTextColor(Color.parseColor("#7CC0FF"))
            gravity = Gravity.END
        }
        top.addView(tvTitle, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        top.addView(tvVal, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT))

        val seek = SeekBar(this).apply {
            max = maxVal - minVal
            progress = (initVal - minVal).coerceIn(0, max)
            setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(sb: SeekBar?, p: Int, fromUser: Boolean) {
                    val v = minVal + p
                    tvVal.text = fmt(v)
                    if (fromUser) onChange(v)
                }
                override fun onStartTrackingTouch(sb: SeekBar?) {}
                override fun onStopTrackingTouch(sb: SeekBar?) {}
            })
        }
        box.addView(top)
        box.addView(seek)
        return box
    }

    private fun switchPreviewTab(to3d: Boolean) {
        is3dTab = to3d
        preview2D.visibility = if (to3d) View.GONE else View.VISIBLE
        preview3D.visibility = if (to3d) View.VISIBLE else View.GONE
        btn2d.background = roundedBg(if (!to3d) "#2E5B88" else "#242935", "#3E475B", 7)
        btn3d.background = roundedBg(if (to3d) "#2E5B88" else "#242935", "#3E475B", 7)
        if (to3d && hasModel && meshDirty) {
            refresh3DMesh()
        }
    }

    private fun setExportEnabled(enabled: Boolean) {
        val a = if (enabled) 1.0f else 0.45f
        btnExportStep.isEnabled = enabled; btnExportStep.alpha = a
        btnExportStl.isEnabled = enabled; btnExportStl.alpha = a
        btnExportSvg.isEnabled = enabled; btnExportSvg.alpha = a
    }

    private fun loadSelectedImage(uri: Uri) {
        lblAudit.text = "⏳ Загрузка изображения..."
        scope.launch {
            val bytes = withContext(Dispatchers.IO) {
                contentResolver.openInputStream(uri)?.use { it.readBytes() }
            }
            if (bytes == null || bytes.isEmpty()) {
                Toast.makeText(this@MainActivity, "Не удалось прочитать файл", Toast.LENGTH_SHORT).show()
                return@launch
            }
            val res = withContext(Dispatchers.Default) { NativeBridge.setImage(bytes) }
            if (!res.startsWith("OK")) {
                lblAudit.text = "⚠ $res"
                return@launch
            }
            val bmp = withContext(Dispatchers.Default) {
                val opts = BitmapFactory.Options().apply { inSampleSize = 2 }
                BitmapFactory.decodeByteArray(bytes, 0, bytes.size, opts)
                    ?: BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
            }
            preview2D.sourceBitmap = bmp
            preview2D.resetView()
            preview3D.resetCamera()
            hasImage = true
            scheduleReprocess(immediate = true)
        }
    }

    private fun buildParamsJson(): String {
        val slotDepth = (standHeightMm * 0.6f).coerceIn(2.5f, 6.0f)
        return JSONObject().apply {
            put("bg_mode", bgMode)
            put("bg_tolerance", bgTolerance)
            put("invert", invert)
            put("contrast", contrast.toDouble())
            put("smooth_strength", smoothStrength)
            put("style_mode", styleMode)
            put("shadow_threshold", shadowThreshold)
            put("edge_sensitivity", edgeSensitivity)
            put("line_thickness_mm", lineThicknessMm.toDouble())
            put("strengthen_outer", strengthenOuter)
            put("outer_contour_mm", outerContourMm.toDouble())
            put("island_mode", islandMode)
            put("bridge_width_mm", bridgeWidthMm.toDouble())
            put("max_bridge_length_mm", maxBridgeLenMm.toDouble())
            put("min_island_area_mm2", minIslandMm2.toDouble())
            put("min_hole_area_mm2", minHoleMm2.toDouble())
            put("wall_offset_mm", wallOffsetMm.toDouble())
            put("prune_spurs_mm", pruneSpursMm.toDouble())
            put("simplify_tol_mm", simplifyTolMm.toDouble())
            put("target_height_mm", targetHeightMm.toDouble())
            put("thickness_mm", thicknessMm.toDouble())
            put("stand_mode", standMode)
            put("add_bottom_bar", addBottomBar)
            put("stand_depth_mm", standDepthMm.toDouble())
            put("stand_height_mm", standHeightMm.toDouble())
            put("slot_depth_mm", slotDepth.toDouble())
            put("slot_clearance_mm", slotClearanceMm.toDouble())
            put("stand_margin_mm", 14.0)
        }.toString()
    }

    private fun scheduleReprocess(immediate: Boolean = false) {
        if (!hasImage) return
        meshDirty = true
        processJob?.cancel()
        processJob = scope.launch {
            if (!immediate) delay(150)
            lblAudit.text = "⏳ Обработка контуров в Rust..."
            val pJson = buildParamsJson()
            val outStr = withContext(Dispatchers.Default) { NativeBridge.processCached(pJson) }
            val obj = JSONObject(outStr)
            if (obj.has("error")) {
                hasModel = false
                setExportEnabled(false)
                val err = obj.getString("error")
                preview2D.setError(err)
                lblAudit.background = roundedBg("#3A1C20", "#8C343D")
                lblAudit.setTextColor(Color.parseColor("#FF9A9A"))
                lblAudit.text = "⚠ $err"
                return@launch
            }

            hasModel = true
            setExportEnabled(true)
            val slotD = (standHeightMm * 0.6f).coerceIn(2.5f, 6.0f)
            preview2D.setModel(obj, standMode, standHeightMm, slotD, addBottomBar)

            val st = obj.getJSONObject("stats")
            lblAudit.background = roundedBg("#163326", "#2B7A53")
            lblAudit.setTextColor(Color.parseColor("#7EF0B5"))
            lblAudit.text =
                "✓ Готово к 3D-печати  |  Тел: ${st.getInt("connected_components")}  |  " +
                "Незамкнутых: ${st.getInt("open_contours")}  |  " +
                "Объектов внутри: ${st.getInt("islands_in_holes_remaining")}  |  " +
                "Мостов: ${st.getInt("bridges_added")}  |  " +
                "Удалено: ${st.getInt("islands_removed")}  |  " +
                "${st.getDouble("width_mm")}×${st.getDouble("height_mm")}×${st.getDouble("thickness_mm")} мм"

            if (is3dTab) refresh3DMesh()
        }
    }

    private fun refresh3DMesh() {
        preview3D.setStatus("⏳ Построение 3D-предосмотра...")
        scope.launch {
            val bytes = withContext(Dispatchers.Default) { NativeBridge.getMesh3d() }
            meshDirty = false
            preview3D.setMeshBytes(bytes)
        }
    }

    private fun performExport(uri: Uri, kind: String) {
        if (!hasModel) return
        Toast.makeText(this, "Генерация ${kind.uppercase()}...", Toast.LENGTH_SHORT).show()
        scope.launch {
            val data = withContext(Dispatchers.Default) {
                when (kind) {
                    "stl" -> NativeBridge.exportStl()
                    "svg" -> NativeBridge.exportSvg()
                    else -> NativeBridge.exportStep()
                }
            }
            if (data.isEmpty()) {
                Toast.makeText(this@MainActivity, "Ошибка генерации файла", Toast.LENGTH_LONG).show()
                return@launch
            }
            val ok = withContext(Dispatchers.IO) {
                try {
                    contentResolver.openOutputStream(uri)?.use { it.write(data) }
                    true
                } catch (_: Exception) {
                    false
                }
            }
            val kb = data.size / 1024
            Toast.makeText(
                this@MainActivity,
                if (ok) "✓ Файл ${kind.uppercase()} сохранён ($kb КБ)" else "Ошибка записи файла",
                Toast.LENGTH_LONG
            ).show()
        }
    }
}
