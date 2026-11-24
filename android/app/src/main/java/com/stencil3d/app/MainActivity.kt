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
    private var pruneSpursMm = 0.3f
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
    private lateinit var btnExportStep: Button
    private lateinit var btnExportStl: Button
    private lateinit var settingsScroll: ScrollView

    private val pickPhotoLauncher = registerForActivityResult(ActivityResultContracts.GetContent()) { uri: Uri? ->
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

    private fun buildRootLayout(): View {
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#1B1E26"))
            setPadding(dp(8), dp(8), dp(8), dp(8))
        }

        // 1. Верхняя панель кнопок
        val topBar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        val btnOpen = makeBtn("📂 Фото", "#2A303E") { pickPhotoLauncher.launch("image/*") }
        btnExportStl = makeBtn("STL", "#2A303E") {
            pendingExportKind = "stl"
            saveFileLauncher.launch("stencil_3d.stl")
        }.apply { isEnabled = false }
        btnExportStep = makeBtn("💾 STEP", "#169C5D") {
            pendingExportKind = "step"
            saveFileLauncher.launch("stencil_3d.step")
        }.apply { isEnabled = false }

        topBar.addView(btnOpen, LinearLayout.LayoutParams(0, dp(42), 1.2f).apply { marginEnd = dp(6) })
        topBar.addView(btnExportStl, LinearLayout.LayoutParams(0, dp(42), 0.7f).apply { marginEnd = dp(6) })
        topBar.addView(btnExportStep, LinearLayout.LayoutParams(0, dp(42), 1.3f))
        root.addView(topBar)

        // 2. Баннер проверки топологии
        lblAudit = TextView(this).apply {
            text = "Выберите фото — здесь появится проверка замкнутости и островков"
            textSize = 12f
            setTextColor(Color.parseColor("#C2CDE0"))
            setPadding(dp(10), dp(6), dp(10), dp(6))
            background = roundedBg("#232A38", "#384357")
        }
        root.addView(lblAudit, LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT
        ).apply { topMargin = dp(6); bottomMargin = dp(6) })

        // 3. Панель переключения 2D / 3D / режимов просмотра
        val viewBar = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        val btn2d = makeBtn("2D", "#2E3A52") { switchPreviewTab(false) }
        val btn3d = makeBtn("3D", "#242935") { switchPreviewTab(true) }
        val btnMode = makeBtn("Вид: Трафарет", "#242935") { btn ->
            preview2D.viewMode = when (preview2D.viewMode) {
                "stencil" -> { btn.text = "Вид: Наложение"; "overlay" }
                "overlay" -> { btn.text = "Вид: CAD"; "cad" }
                else -> { btn.text = "Вид: Трафарет"; "stencil" }
            }
            preview2D.invalidate()
        }
        val btnFull = makeBtn("⛶", "#242935") {
            settingsScroll.visibility = if (settingsScroll.visibility == View.VISIBLE) View.GONE else View.VISIBLE
        }
        viewBar.addView(btn2d, LinearLayout.LayoutParams(0, dp(36), 0.7f).apply { marginEnd = dp(4) })
        viewBar.addView(btn3d, LinearLayout.LayoutParams(0, dp(36), 0.7f).apply { marginEnd = dp(4) })
        viewBar.addView(btnMode, LinearLayout.LayoutParams(0, dp(36), 1.4f).apply { marginEnd = dp(4) })
        viewBar.addView(btnFull, LinearLayout.LayoutParams(dp(44), dp(36)))
        root.addView(viewBar)

        // 4. Область предосмотра (2D + 3D)
        val previewFrame = FrameLayout(this).apply {
            background =
