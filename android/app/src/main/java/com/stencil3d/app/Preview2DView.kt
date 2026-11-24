package com.stencil3d.app

import android.content.Context
import android.graphics.*
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.View
import org.json.JSONArray
import org.json.JSONObject
import kotlin.math.max
import kotlin.math.min

class Preview2DView(context: Context) : View(context) {
    private var outer: FloatArray? = null
    private var holes: List<FloatArray> = emptyList()
    private var bridges: List<FloatArray> = emptyList()
    private var removed: List<FloatArray> = emptyList()
    private var bottomBar: FloatArray? = null
    private var standBoxX = floatArrayOf(-50f, 50f)
    private var imgBounds = floatArrayOf(-50f, 0f, 50f, 100f)
    private var bounds = floatArrayOf(-50f, 0f, 50f, 100f)
    private var standHeightMm = 10f
    private var slotDepthMm = 6f
    private var standMode = "slotted"
    private var addBottomBar = true

    var sourceBitmap: Bitmap? = null
    var viewMode = "stencil" // "stencil", "overlay", "cad"
    var showBridges = true
    var showRemoved = true
    var showStand = true

    private var zoom = 1f
    private var panX = 0f
    private var panY = 0f
    private var lastX = 0f
    private var lastY = 0f
    private var lastTapMs = 0L
    private var placeholder = "Нажмите «📂 Фото» сверху,\nчтобы выбрать картинку"

    private val scaleDetector = ScaleGestureDetector(context, object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
        override fun onScale(d: ScaleGestureDetector): Boolean {
            zoom = (zoom * d.scaleFactor).coerceIn(0.4f, 10f)
            invalidate()
            return true
        }
    })

    private val paintFill = Paint(Paint.ANTI_ALIAS_FLAG)
    private val paintStroke = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
    private val paintText = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        textSize = 30f
        textAlign = Paint.Align.CENTER
    }
    private val paintAlpha = Paint(Paint.ANTI_ALIAS_FLAG).apply { alpha = 115 }

    fun setError(msg: String) {
        outer = null
        placeholder = "⚠ $msg"
        invalidate()
    }

    fun resetView() {
        zoom = 1f
        panX = 0f
        panY = 0f
        invalidate()
    }

    fun setModel(json: JSONObject, standMode: String, standH: Float, slotD: Float, addBar: Boolean) {
        this.standMode = standMode
        this.standHeightMm = standH
        this.slotDepthMm = slotD
        this.addBottomBar = addBar
        outer = parseRing(json.getJSONArray("outer"))
        holes = parseRingList(json.getJSONArray("holes"))
        bridges = parseRingList(json.getJSONArray("bridges"))
        removed = parseRingList(json.getJSONArray("removed"))
        bottomBar = if (!json.isNull("bottom_bar")) parseArr4(json.getJSONArray("bottom_bar")) else null
        val sb = json.getJSONArray("stand_box_x")
        standBoxX = floatArrayOf(sb.getDouble(0).toFloat(), sb.getDouble(1).toFloat())
        imgBounds = parseArr4(json.getJSONArray("img_bounds_mm"))
        bounds = parseArr4(json.getJSONArray("bounds_mm"))
        invalidate()
    }

    private fun parseArr4(a: JSONArray) = floatArrayOf(
        a.getDouble(0).toFloat(), a.getDouble(1).toFloat(),
        a.getDouble(2).toFloat(), a.getDouble(3).toFloat()
    )

    private fun parseRing(a: JSONArray): FloatArray {
        val out = FloatArray(a.length() * 2)
        for (i in 0 until a.length()) {
            val pt = a.getJSONArray(i)
            out[i * 2] = pt.getDouble(0).toFloat()
            out[i * 2 + 1] = pt.getDouble(1).toFloat()
        }
        return out
    }

    private fun parseRingList(a: JSONArray): List<FloatArray> {
        val list = ArrayList<FloatArray>(a.length())
        for (i in 0 until a.length()) list.add(parseRing(a.getJSONArray(i)))
        return list
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val lightBg = viewMode == "stencil"
        canvas.drawColor(if (lightBg) Color.rgb(232, 236, 242) else Color.rgb(26, 30, 38))

        val outRing = outer
        if (outRing == null || outRing.size < 6) {
            paintText.color = if (lightBg) Color.DKGRAY else Color.LTGRAY
            paintText.textSize = 36f
            val lines = placeholder.split("\n")
            val yStart = height * 0.5f - (lines.size - 1) * 24f
            lines.forEachIndexed { idx, line ->
                canvas.drawText(line, width * 0.5f, yStart + idx * 48f, paintText)
            }
            return
        }

        var minX = bounds[0]
        var minY = bounds[1]
        var maxX = bounds[2]
        var maxY = bounds[3]
        if (showStand && standMode != "none") {
            minX = min(minX, standBoxX[0])
            maxX = max(maxX, standBoxX[1])
            minY = min(minY, -standHeightMm - 6f)
        }

        val bw = max(10f, maxX - minX)
        val bh = max(10f, maxY - minY)
        val pad = 42f
        val sc = min((width - 2 * pad) / bw, (height - 2 * pad) / bh) * zoom
        val cxMm = 0.5f * (minX + maxX)
        val cyMm = 0.5f * (minY + maxY)
        val ox = width * 0.5f + panX
        val oy = height * 0.5f + panY

        fun sx(x: Float) = ox + (x - cxMm) * sc
        fun sy(y: Float) = oy - (y - cyMm) * sc

        fun buildPath(ring: FloatArray, target: Path) {
            if (ring.size < 6) return
            target.moveTo(sx(ring[0]), sy(ring[1]))
            var i = 2
            while (i < ring.size) {
                target.lineTo(sx(ring[i]), sy(ring[i + 1]))
                i += 2
            }
            target.close()
        }

        // Полупрозрачный оригинал (режим «Наложение»)
        if (viewMode == "overlay") {
            sourceBitmap?.let { bmp ->
                val dst = RectF(sx(imgBounds[0]), sy(imgBounds[3]), sx(imgBounds[2]), sy(imgBounds[1]))
                canvas.drawBitmap(bmp, null, dst, paintAlpha)
            }
        }

        // Удалённые островки (красный)
        if (showRemoved && removed.isNotEmpty()) {
            paintFill.style = Paint.Style.FILL
            paintFill.color = Color.argb(120, 235, 64, 64)
            paintStroke.color = Color.argb(210, 255, 95, 95)
            paintStroke.strokeWidth = 2f
            for (r in removed) {
                val p = Path()
                buildPath(r, p)
                canvas.drawPath(p, paintFill)
                canvas.drawPath(p, paintStroke)
            }
        }

        // Основной замкнутый силуэт
        val mainPath = Path().apply { fillType = Path.FillType.EVEN_ODD }
        buildPath(outRing, mainPath)
        for (h in holes) buildPath(h, mainPath)

        if (viewMode == "cad") {
            paintFill.style = Paint.Style.FILL
            paintFill.color = Color.argb(55, 50, 155, 255)
            canvas.drawPath(mainPath, paintFill)
            paintStroke.color = Color.rgb(80, 220, 255)
            paintStroke.strokeWidth = 3f
            val op = Path(); buildPath(outRing, op)
            canvas.drawPath(op, paintStroke)
            paintStroke.color = Color.rgb(255, 180, 70)
            paintStroke.strokeWidth = 2.2f
            for (h in holes) {
                val hp = Path(); buildPath(h, hp)
                canvas.drawPath(hp, paintStroke)
            }
        } else {
            paintFill.style = Paint.Style.FILL
            paintFill.color = if (viewMode == "overlay") Color.argb(210, 20, 23, 28) else Color.rgb(22, 25, 31)
            canvas.drawPath(mainPath, paintFill)
        }

        // Нижняя планка в паз
        if (showStand) {
            bottomBar?.let { b ->
                paintFill.color = Color.argb(130, 245, 185, 66)
                canvas.drawRect(sx(b[0]), sy(b[3]), sx(b[2]), sy(b[1]), paintFill)
            }
        }

        // Авто-мосты (бирюзовый)
        if (showBridges && bridges.isNotEmpty()) {
            paintFill.style = Paint.Style.FILL
            paintFill.color = Color.argb(200, 0, 225, 205)
            for (br in bridges) {
                val p = Path()
                buildPath(br, p)
                canvas.drawPath(p, paintFill)
            }
        }

        // Силуэт ступенчатой подставки
        if (showStand && standMode != "none") {
            val h1 = standHeightMm * 0.44f
            val inset = min(3.5f, (standBoxX[1] - standBoxX[0]) * 0.08f)
            val yTop = if (addBottomBar) slotDepthMm else 0f
            val yBot = yTop - standHeightMm
            paintFill.color = Color.argb(55, 255, 185, 55)
            paintStroke.color = Color.rgb(255, 190, 75)
            paintStroke.strokeWidth = 2.5f
            val rLow = RectF(sx(standBoxX[0]), sy(yBot + h1), sx(standBoxX[1]), sy(yBot))
            val rUp = RectF(sx(standBoxX[0] + inset), sy(yTop), sx(standBoxX[1] - inset), sy(yBot + h1))
            canvas.drawRect(rLow, paintFill); canvas.drawRect(rLow, paintStroke)
            canvas.drawRect(rUp, paintFill); canvas.drawRect(rUp, paintStroke)
        }

        // Габариты
        paintText.textSize = 26f
        paintText.color = if (lightBg) Color.rgb(55, 65, 80) else Color.rgb(190, 205, 225)
        val wMm = bounds[2] - bounds[0]
        val hMm = bounds[3] - bounds[1]
        canvas.drawText(
            String.format("%.1f × %.1f мм  |  2×тап: сброс", wMm, hMm),
            width * 0.5f,
            height - 16f,
            paintText
        )
    }

    override fun onTouchEvent(ev: MotionEvent): Boolean {
        scaleDetector.onTouchEvent(ev)
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                val now = System.currentTimeMillis()
                if (now - lastTapMs < 280) resetView()
                lastTapMs = now
                lastX = ev.x; lastY = ev.y
                parent?.requestDisallowInterceptTouchEvent(true)
            }
            MotionEvent.ACTION_MOVE -> {
                if (!scaleDetector.isInProgress && ev.pointerCount == 1) {
                    panX += ev.x - lastX
                    panY += ev.y - lastY
                    lastX = ev.x; lastY = ev.y
                    invalidate()
                }
            }
        }
        return true
    }
}
